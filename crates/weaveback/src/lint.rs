use std::fs;
use std::path::{Path, PathBuf};
use weaveback_tangle::NowebSyntax;

#[derive(serde::Deserialize)]
struct LintPassCfg {
    open_delim: Option<String>,
    close_delim: Option<String>,
    chunk_end: Option<String>,
    comment_markers: Option<String>,
}

#[derive(serde::Deserialize)]
struct LintCfg {
    #[serde(rename = "pass", default)]
    passes: Vec<LintPassCfg>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LintRule {
    ChunkBodyOutsideFence,
}

impl LintRule {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::ChunkBodyOutsideFence => "chunk-body-outside-fence",
        }
    }
}

impl std::str::FromStr for LintRule {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "chunk-body-outside-fence" => Ok(Self::ChunkBodyOutsideFence),
            _ => Err(format!(
                "unknown lint rule '{s}' (supported: chunk-body-outside-fence)"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LintViolation {
    pub(crate) file: PathBuf,
    pub(crate) line: usize,
    pub(crate) rule: LintRule,
    pub(crate) message: String,
    pub(crate) hint: Option<String>,
}

fn should_skip_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    matches!(
        name,
        ".git" | "target" | "node_modules" | ".venv" | ".plantuml-cache" | "__pycache__"
    ) || path.ends_with("docs/html")
}

fn collect_adoc_files(path: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_file() {
        if path.extension().is_some_and(|e| e == "adoc") {
            out.push(path.to_path_buf());
        }
        return Ok(());
    }
    if !path.is_dir() || should_skip_dir(path) {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() {
            if should_skip_dir(&child) {
                continue;
            }
            collect_adoc_files(&child, out)?;
        } else if child.extension().is_some_and(|e| e == "adoc") {
            out.push(child);
        }
    }
    Ok(())
}

fn load_lint_syntaxes() -> Vec<NowebSyntax> {
    let mut syntaxes = vec![NowebSyntax::new(
        "<<",
        ">>",
        "@",
        &["#".to_string(), "//".to_string()],
    )];

    let Ok(src) = fs::read_to_string("weaveback.toml") else {
        return syntaxes;
    };
    let Ok(cfg) = toml::from_str::<LintCfg>(&src) else {
        return syntaxes;
    };

    for pass in cfg.passes {
        let open_delim = pass.open_delim.unwrap_or_else(|| "<<".to_string());
        let close_delim = pass.close_delim.unwrap_or_else(|| ">>".to_string());
        let chunk_end = pass.chunk_end.unwrap_or_else(|| "@".to_string());
        let comment_markers = pass
            .comment_markers
            .as_deref()
            .unwrap_or("#,//")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        syntaxes.push(NowebSyntax::new(
            &open_delim,
            &close_delim,
            &chunk_end,
            &comment_markers,
        ));
    }

    syntaxes
}
fn parse_chunk_definition_name(line: &str, syntaxes: &[NowebSyntax]) -> Option<String> {
    for syntax in syntaxes {
        if let Some(def_match) = syntax.parse_definition_line(line) {
            return Some(def_match.base_name);
        }
    }
    None
}

fn lint_chunk_body_outside_fence(
    file: &Path,
    text: &str,
    syntaxes: &[NowebSyntax],
) -> Vec<LintViolation> {
    let mut in_fence = false;
    let mut violations = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == "----" {
            in_fence = !in_fence;
            continue;
        }
        if let Some(chunk_name) =
            parse_chunk_definition_name(trimmed, syntaxes).filter(|_| !in_fence)
        {
            violations.push(LintViolation {
                file: file.to_path_buf(),
                line: idx + 1,
                rule: LintRule::ChunkBodyOutsideFence,
                message: format!(
                    "chunk definition `{chunk_name}` is not enclosed by a fenced code block"
                ),
                hint: Some(
                    "wrap the chunk definition in an AsciiDoc listing block fenced by `----`"
                        .to_string(),
                ),
            });
        }
    }
    violations
}

pub(crate) fn lint_paths(
    paths: &[PathBuf],
    rule_filter: Option<LintRule>,
) -> Result<Vec<LintViolation>, String> {
    let mut adoc_files = Vec::new();
    let syntaxes = load_lint_syntaxes();
    if paths.is_empty() {
        collect_adoc_files(Path::new("."), &mut adoc_files).map_err(|e| e.to_string())?;
    } else {
        for path in paths {
            collect_adoc_files(path, &mut adoc_files).map_err(|e| e.to_string())?;
        }
    }
    adoc_files.sort();
    adoc_files.dedup();

    let mut violations = Vec::new();
    for file in adoc_files {
        let text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        if rule_filter.is_none() || rule_filter == Some(LintRule::ChunkBodyOutsideFence) {
            violations.extend(lint_chunk_body_outside_fence(&file, &text, &syntaxes));
        }
    }
    Ok(violations)
}
pub(crate) fn run_lint(
    paths: Vec<PathBuf>,
    strict: bool,
    rule: Option<String>,
) -> Result<(), String> {
    let rule_filter = match rule {
        Some(rule) => Some(rule.parse::<LintRule>()?),
        None => None,
    };

    let violations = lint_paths(&paths, rule_filter)?;
    if violations.is_empty() {
        println!("lint: no violations");
        return Ok(());
    }

    for v in &violations {
        println!("{}:{}: {}", v.file.display(), v.line, v.rule.id());
        println!("  {}", v.message);
        if let Some(hint) = &v.hint {
            println!("  hint: {hint}");
        }
    }

    let summary = format!("lint: {} violation(s)", violations.len());
    if strict {
        Err(summary)
    } else {
        eprintln!("{summary}");
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn lint_detects_chunk_outside_fence() {
        let text = "= Title\n\n// <<alpha>>=\nbody\n// @\n";
        let syntaxes = vec![NowebSyntax::new(
            "<<",
            ">>",
            "@",
            &["#".to_string(), "//".to_string()],
        )];
        let violations = lint_chunk_body_outside_fence(Path::new("sample.adoc"), text, &syntaxes);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line, 3);
        assert_eq!(violations[0].rule, LintRule::ChunkBodyOutsideFence);
    }

    #[test]
    fn lint_accepts_chunk_inside_fence() {
        let text = "= Title\n\n----\n// <<alpha>>=\nbody\n// @\n----\n";
        let syntaxes = vec![NowebSyntax::new(
            "<<",
            ">>",
            "@",
            &["#".to_string(), "//".to_string()],
        )];
        assert!(lint_chunk_body_outside_fence(Path::new("sample.adoc"), text, &syntaxes).is_empty());
    }

    #[test]
    fn lint_detects_hash_prefixed_angle_bracket_chunks() {
        let text = "= Title\n\n# <[alpha]>=\nbody\n# @@\n";
        let syntaxes = vec![NowebSyntax::new(
            "<[",
            "]>",
            "@@",
            &["#".to_string(), "//".to_string()],
        )];
        let violations = lint_chunk_body_outside_fence(Path::new("sample.adoc"), text, &syntaxes);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("alpha"));
    }

    #[test]
    fn lint_detects_uncommented_triple_angle_chunks() {
        let text = "= Title\n\n<<<alpha>>>=\nbody\n@@\n";
        let syntaxes = vec![NowebSyntax::new("<<<", ">>>", "@@", &["//".to_string()])];
        let violations = lint_chunk_body_outside_fence(Path::new("sample.adoc"), text, &syntaxes);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("alpha"));
    }

    #[test]
    fn load_lint_syntaxes_reads_pass_syntax_from_config() {
        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("weaveback.toml"),
            r#"
[[pass]]
dir = "demo/"
open_delim = "<["
close_delim = "]>"
chunk_end = "@@"
comment_markers = "//"
"#,
        )
        .unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();
        let syntaxes = load_lint_syntaxes();
        std::env::set_current_dir(cwd).unwrap();

        assert!(
            parse_chunk_definition_name("// <[alpha]>=", &syntaxes)
                .as_deref()
                == Some("alpha")
        );
    }

    #[test]
    fn collect_adoc_files_skips_generated_dirs() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join("docs/html")).unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src").join("ok.adoc"), "= Ok\n").unwrap();
        fs::write(temp.path().join("docs/html").join("skip.adoc"), "= Skip\n").unwrap();

        let mut files = Vec::new();
        collect_adoc_files(temp.path(), &mut files).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("ok.adoc"));
    }

    #[test]
    fn run_lint_is_warning_by_default_and_error_in_strict_mode() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("bad.adoc");
        fs::write(&path, "// <<alpha>>=\nbody\n// @\n").unwrap();

        assert!(run_lint(vec![path.clone()], false, None).is_ok());
        let err = run_lint(vec![path], true, None).unwrap_err();
        assert!(err.contains("1 violation"));
    }
}
