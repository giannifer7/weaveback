# Structural Lint

`lint.rs` is a structural linter for literate sources.
The point is not generic style enforcement.  The point is to make project
invariants explicit and cheap to check.

Two rules are implemented:

`chunk-body-outside-fence`::
  A chunk definition that appears outside an AsciiDoc listing block is likely
  a mistake — it will not be extracted by the tangle pass.

`unterminated-chunk-definition`::
  A chunk whose opening `<[name]>=` marker is never followed by the configured
  chunk-end marker (e.g. `// @`) will silently swallow content.

## Core Types


```rust
// <[lint-core-types]>=
use std::fs;
use std::path::{Path, PathBuf};
use weaveback_tangle::NowebSyntax;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum LintRule {
    ChunkBodyOutsideFence,
    UnterminatedChunkDefinition,
}

impl LintRule {
    pub fn id(self) -> &'static str {
        match self {
            Self::ChunkBodyOutsideFence => "chunk-body-outside-fence",
            Self::UnterminatedChunkDefinition => "unterminated-chunk-definition",
        }
    }
}

impl std::str::FromStr for LintRule {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "chunk-body-outside-fence" => Ok(Self::ChunkBodyOutsideFence),
            "unterminated-chunk-definition" => Ok(Self::UnterminatedChunkDefinition),
            _ => Err(format!(
                "unknown lint rule '{s}' (supported: chunk-body-outside-fence, unterminated-chunk-definition)"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LintViolation {
    pub file:    PathBuf,
    pub line:    usize,
    pub rule:    LintRule,
    pub message: String,
    pub hint:    Option<String>,
}
// @
```


## Filesystem Scanning

`collect_adoc_files` recursively walks a directory collecting `.adoc` files.
`should_skip_dir` prunes well-known generated or dependency directories that
should never be linted.


```rust
// <[lint-fs]>=
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
// @
```


## Configuration and Syntax Loading

`load_lint_syntaxes_from` reads `weaveback.toml` and constructs one
`LintSyntaxEntry` per `[[pass]]` section, plus a default entry for
the standard `<<`/`>>` syntax.  `lint_syntaxes_for_file` selects the
entries whose `dir` prefix matches a given file path.


```rust
// <[lint-config]>=
#[derive(serde::Deserialize)]
struct LintPassCfg {
    dir:             Option<String>,
    open_delim:      Option<String>,
    close_delim:     Option<String>,
    chunk_end:       Option<String>,
    comment_markers: Option<String>,
}

#[derive(serde::Deserialize)]
struct LintCfg {
    #[serde(rename = "pass", default)]
    passes: Vec<LintPassCfg>,
}

#[derive(Clone)]
struct LintSyntaxEntry {
    dir:    Option<PathBuf>,
    syntax: NowebSyntax,
}

fn load_lint_syntaxes_from(base_dir: &Path) -> Vec<LintSyntaxEntry> {
    let mut syntaxes = vec![LintSyntaxEntry {
        dir: None,
        syntax: NowebSyntax::new(
            "<<",
            ">>",
            "@",
            &["#".to_string(), "//".to_string()],
        ),
    }];

    let Ok(src) = fs::read_to_string(base_dir.join("weaveback.toml")) else {
        return syntaxes;
    };
    let Ok(cfg) = toml::from_str::<LintCfg>(&src) else {
        return syntaxes;
    };

    for pass in cfg.passes {
        let open_delim      = pass.open_delim .unwrap_or_else(|| "<<".to_string());
        let close_delim     = pass.close_delim.unwrap_or_else(|| ">>".to_string());
        let chunk_end       = pass.chunk_end  .unwrap_or_else(|| "@".to_string());
        let comment_markers = pass
            .comment_markers
            .as_deref()
            .unwrap_or("#,//")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        syntaxes.push(LintSyntaxEntry {
            dir: pass.dir.map(PathBuf::from),
            syntax: NowebSyntax::new(&open_delim, &close_delim, &chunk_end, &comment_markers),
        });
    }

    syntaxes
}

fn load_lint_syntaxes() -> Vec<LintSyntaxEntry> {
    load_lint_syntaxes_from(Path::new("."))
}

fn lint_syntaxes_for_file<'a>(file: &Path, syntaxes: &'a [LintSyntaxEntry]) -> Vec<&'a NowebSyntax> {
    let rel = file.strip_prefix(".").unwrap_or(file);
    let mut matched = syntaxes
        .iter()
        .filter(|entry| entry.dir.as_ref().is_some_and(|dir| rel.starts_with(dir)))
        .map(|entry| &entry.syntax)
        .collect::<Vec<_>>();

    if matched.is_empty() {
        matched.extend(
            syntaxes
                .iter()
                .filter(|entry| entry.dir.is_none())
                .map(|entry| &entry.syntax),
        );
    }

    matched
}
// @
```


## Rules


```rust
// <[lint-rules]>=
fn parse_chunk_definition_name(line: &str, syntaxes: &[&NowebSyntax]) -> Option<String> {
    for syntax in syntaxes {
        if let Some(def_match) = syntax.parse_definition_line(line) {
            return Some(def_match.base_name);
        }
    }
    None
}

fn parse_chunk_definition(line: &str, syntaxes: &[&NowebSyntax]) -> Option<(String, usize)> {
    for (idx, syntax) in syntaxes.iter().enumerate() {
        if let Some(def_match) = syntax.parse_definition_line(line) {
            return Some((def_match.base_name, idx));
        }
    }
    None
}

fn lint_chunk_body_outside_fence(
    file: &Path,
    text: &str,
    syntaxes: &[&NowebSyntax],
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

fn lint_unterminated_chunk_definition(
    file: &Path,
    text: &str,
    syntaxes: &[&NowebSyntax],
) -> Vec<LintViolation> {
    let mut violations = Vec::new();
    let mut active: Option<(usize, String, usize)> = None;

    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();

        if let Some((syntax_idx, _, _)) = &active
            && syntaxes[*syntax_idx].is_close_line(trimmed)
        {
            active = None;
            continue;
        }

        if let Some((chunk_name, syntax_idx)) = parse_chunk_definition(trimmed, syntaxes) {
            if let Some((_, prev_name, prev_line)) = active.take() {
                violations.push(LintViolation {
                    file: file.to_path_buf(),
                    line: prev_line + 1,
                    rule: LintRule::UnterminatedChunkDefinition,
                    message: format!(
                        "chunk definition `{prev_name}` is not closed before a new chunk definition starts"
                    ),
                    hint: Some(
                        "add the configured chunk-end marker (for example `// @`) before the next chunk definition"
                            .to_string(),
                    ),
                });
            }
            active = Some((syntax_idx, chunk_name, idx));
        }
    }

    if let Some((_, chunk_name, line)) = active {
        violations.push(LintViolation {
            file: file.to_path_buf(),
            line: line + 1,
            rule: LintRule::UnterminatedChunkDefinition,
            message: format!("chunk definition `{chunk_name}` reaches end of file without a closing marker"),
            hint: Some(
                "add the configured chunk-end marker (for example `// @`) before end of file"
                    .to_string(),
            ),
        });
    }

    violations
}

pub fn lint_paths(
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
        let file_syntaxes = lint_syntaxes_for_file(&file, &syntaxes);
        if rule_filter.is_none() || rule_filter == Some(LintRule::ChunkBodyOutsideFence) {
            violations.extend(lint_chunk_body_outside_fence(&file, &text, &file_syntaxes));
        }
        if rule_filter.is_none() || rule_filter == Some(LintRule::UnterminatedChunkDefinition) {
            violations.extend(lint_unterminated_chunk_definition(&file, &text, &file_syntaxes));
        }
    }
    Ok(violations)
}
// @
```


## Entry Point


```rust
// <[lint-run]>=
pub fn run_lint(
    paths: Vec<PathBuf>,
    strict: bool,
    rule: Option<String>,
    json_output: bool,
) -> Result<(), String> {
    let rule_filter = match rule {
        Some(rule) => Some(rule.parse::<LintRule>()?),
        None => None,
    };

    let violations = lint_paths(&paths, rule_filter)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": violations.is_empty(),
                "count": violations.len(),
                "violations": violations,
            }))
            .unwrap()
        );
        if strict && !violations.is_empty() {
            return Err(format!("lint: {} violation(s)", violations.len()));
        }
        return Ok(());
    }
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
// @
```


## Tests


```rust
// <[@file weaveback-api/src/lint/tests.rs]>=
// weaveback-api/src/lint/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use tempfile::TempDir;

#[test]
fn lint_detects_chunk_outside_fence() {
    let text = "= Title\n\n// <<alpha>>=\nbody\n// @\n";
    let syntax = NowebSyntax::new(
        "<<",
        ">>",
        "@",
        &["#".to_string(), "//".to_string()],
    );
    let syntaxes = vec![&syntax];
    let violations = lint_chunk_body_outside_fence(Path::new("sample.adoc"), text, &syntaxes);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].line, 3);
    assert_eq!(violations[0].rule, LintRule::ChunkBodyOutsideFence);
}

#[test]
fn lint_accepts_chunk_inside_fence() {
    let text = "= Title\n\n----\n// <<alpha>>=\nbody\n// @\n----\n";
    let syntax = NowebSyntax::new(
        "<<",
        ">>",
        "@",
        &["#".to_string(), "//".to_string()],
    );
    let syntaxes = vec![&syntax];
    assert!(lint_chunk_body_outside_fence(Path::new("sample.adoc"), text, &syntaxes).is_empty());
}

#[test]
fn lint_detects_unterminated_chunk_at_end_of_file() {
    let text = "= Title\n\n----\n// <<alpha>>=\nbody\n----\n";
    let syntax = NowebSyntax::new(
        "<<",
        ">>",
        "@",
        &["#".to_string(), "//".to_string()],
    );
    let syntaxes = vec![&syntax];
    let violations =
        lint_unterminated_chunk_definition(Path::new("sample.adoc"), text, &syntaxes);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].line, 4);
    assert_eq!(violations[0].rule, LintRule::UnterminatedChunkDefinition);
}

#[test]
fn lint_detects_new_chunk_before_previous_close() {
    let text = "= Title\n\n----\n// <<alpha>>=\nbody\n// <<beta>>=\nmore\n// @\n----\n";
    let syntax = NowebSyntax::new(
        "<<",
        ">>",
        "@",
        &["#".to_string(), "//".to_string()],
    );
    let syntaxes = vec![&syntax];
    let violations =
        lint_unterminated_chunk_definition(Path::new("sample.adoc"), text, &syntaxes);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].message.contains("alpha"));
}

#[test]
fn lint_detects_hash_prefixed_angle_bracket_chunks() {
    let text = "= Title\n\n# <[alpha]>=\nbody\n# @@\n";
    let syntax = NowebSyntax::new(
        "<[",
        "]>",
        "@@",
        &["#".to_string(), "//".to_string()],
    );
    let syntaxes = vec![&syntax];
    let violations = lint_chunk_body_outside_fence(Path::new("sample.adoc"), text, &syntaxes);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].message.contains("alpha"));
}

#[test]
fn lint_detects_uncommented_triple_angle_chunks() {
    let text = "= Title\n\n<<<alpha>>>=\nbody\n@@\n";
    let syntax = NowebSyntax::new("<<<", ">>>", "@@", &["//".to_string()]);
    let syntaxes = vec![&syntax];
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
    let syntaxes = load_lint_syntaxes_from(temp.path());
    let syntaxes = syntaxes.iter().map(|entry| &entry.syntax).collect::<Vec<_>>();

    assert!(
        parse_chunk_definition_name("// <[alpha]>=", &syntaxes)
            .as_deref()
            == Some("alpha")
    );
}

#[test]
fn lint_uses_matching_pass_syntax_only() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("weaveback.toml"),
        r#"
[[pass]]
dir = "examples/"
open_delim = "<["
close_delim = "]>"
chunk_end = "@@"
comment_markers = "//"
"#,
    )
    .unwrap();
    fs::create_dir_all(temp.path().join("examples")).unwrap();
    let file = temp.path().join("README.adoc");
    fs::write(&file, "----\n// <[alpha]>=\nbody\n// @\n----\n").unwrap();

    let syntaxes = load_lint_syntaxes_from(temp.path());
    let file_syntaxes = lint_syntaxes_for_file(Path::new("./README.adoc"), &syntaxes);

    assert!(parse_chunk_definition_name("// <[alpha]>=", &file_syntaxes).is_none());
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

    assert!(run_lint(vec![path.clone()], false, None, false).is_ok());
    let err = run_lint(vec![path], true, None, false).unwrap_err();
    assert!(err.contains("1 violation"));
}

#[test]
fn run_lint_can_emit_json() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("bad.adoc");
    fs::write(&path, "// <<alpha>>=\nbody\n// @\n").unwrap();

    assert!(run_lint(vec![path], false, None, true).is_ok());
}

// @
```


## Assembly


```rust
// <[@file weaveback-api/src/lint.rs]>=
// weaveback-api/src/lint.rs
// I'd Really Rather You Didn't edit this generated file.

// <[lint-core-types]>
// <[lint-fs]>
// <[lint-config]>
// <[lint-rules]>
// <[lint-run]>
#[cfg(test)]
mod tests;

// @
```

