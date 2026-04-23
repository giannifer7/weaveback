// weaveback-api/src/lint.rs
// I'd Really Rather You Didn't edit this generated file.

use std::fs;
use std::path::{Path, PathBuf};
use weaveback_tangle::NowebSyntax;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum LintRule {
    ChunkBodyOutsideFence,
    UnterminatedChunkDefinition,
    RawWvbLink,
    RawWvbSourceBlock,
    RawWvbTable,
}

impl LintRule {
    pub fn id(self) -> &'static str {
        match self {
            Self::ChunkBodyOutsideFence => "chunk-body-outside-fence",
            Self::UnterminatedChunkDefinition => "unterminated-chunk-definition",
            Self::RawWvbLink => "raw-wvb-link",
            Self::RawWvbSourceBlock => "raw-wvb-source-block",
            Self::RawWvbTable => "raw-wvb-table",
        }
    }
}

impl std::str::FromStr for LintRule {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "chunk-body-outside-fence" => Ok(Self::ChunkBodyOutsideFence),
            "unterminated-chunk-definition" => Ok(Self::UnterminatedChunkDefinition),
            "raw-wvb-link" => Ok(Self::RawWvbLink),
            "raw-wvb-source-block" => Ok(Self::RawWvbSourceBlock),
            "raw-wvb-table" => Ok(Self::RawWvbTable),
            _ => Err(format!(
                "unknown lint rule '{s}' (supported: chunk-body-outside-fence, unterminated-chunk-definition, raw-wvb-link, raw-wvb-source-block, raw-wvb-table)"
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
fn should_skip_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    matches!(
        name,
        ".git"
            | "target"
            | "node_modules"
            | ".venv"
            | ".plantuml-cache"
            | "__pycache__"
            | "expanded-adoc"
            | "expanded-md"
    ) || path.ends_with("docs/html")
}

fn is_lint_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| matches!(ext, "adoc" | "wvb"))
}

fn is_wvb_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("wvb")
}

fn is_prelude_file(path: &Path) -> bool {
    path.components().any(|component| component.as_os_str() == "prelude")
}

fn collect_literate_files(path: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_file() {
        if is_lint_source_file(path) {
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
            collect_literate_files(&child, out)?;
        } else if is_lint_source_file(&child) {
            out.push(child);
        }
    }
    Ok(())
}
#[derive(serde::Deserialize)]
struct LintPassCfg {
    dir:             Option<String>,
    ext:             Option<String>,
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
    ext:    Option<String>,
    syntax: NowebSyntax,
}

fn load_lint_syntaxes_from(base_dir: &Path) -> Vec<LintSyntaxEntry> {
    let mut syntaxes = vec![LintSyntaxEntry {
        dir: None,
        ext: None,
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
        let ext             = pass.ext;
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
            ext,
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
    let file_ext = file.extension().and_then(|e| e.to_str());
    let mut matched = syntaxes
        .iter()
        .filter(|entry| {
            entry.dir.as_ref().is_some_and(|dir| rel.starts_with(dir))
                && entry.ext.as_deref().is_none_or(|ext| Some(ext) == file_ext)
        })
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

fn is_adoc_fence_delimiter(line: &str) -> bool {
    matches!(line.trim(), "----" | "....")
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
        if is_adoc_fence_delimiter(trimmed) {
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

fn lint_raw_wvb_links(file: &Path, text: &str) -> Vec<LintViolation> {
    if !is_wvb_file(file) || is_prelude_file(file) {
        return Vec::new();
    }

    let mut violations = Vec::new();
    let mut in_generated_code_macro = false;
    for (idx, line) in text.lines().enumerate() {
        if line_opens_generated_code_macro(line) {
            in_generated_code_macro = true;
        }

        if !in_generated_code_macro
            && (line.contains("link:") || line.contains("xref:"))
            && line.contains('[')
            && line.contains(']')
        {
            violations.push(LintViolation {
                file: file.to_path_buf(),
                line: idx + 1,
                rule: LintRule::RawWvbLink,
                message: "raw AsciiDoc link syntax in `.wvb` source".to_string(),
                hint: Some("use `\u{00a4}link(target, label)` or `\u{00a4}xref(target, label)`".to_string()),
            });
        }

        if in_generated_code_macro && line_closes_prelude_block(line) {
            in_generated_code_macro = false;
        }
    }
    violations
}

fn lint_raw_wvb_source_blocks(file: &Path, text: &str) -> Vec<LintViolation> {
    if !is_wvb_file(file) || is_prelude_file(file) {
        return Vec::new();
    }

    let mut violations = Vec::new();
    let mut in_generated_code_macro = false;
    let mut previous_line = "";
    for (idx, line) in text.lines().enumerate() {
        if line_opens_generated_code_macro(line) {
            in_generated_code_macro = true;
        }

        let is_raw_source =
            line.starts_with("[source") || line.starts_with("[plantuml") || line.starts_with("[d2");
        let is_prelude_definition_body = previous_line.contains("\u{00a4}redef(");
        if is_raw_source && !in_generated_code_macro && !is_prelude_definition_body {
            violations.push(LintViolation {
                file: file.to_path_buf(),
                line: idx + 1,
                rule: LintRule::RawWvbSourceBlock,
                message: "raw AsciiDoc source or diagram block marker in `.wvb` source".to_string(),
                hint: Some("use `\u{00a4}code_block(language, body)` or `\u{00a4}graph(format, name, body)`".to_string()),
            });
        }

        if in_generated_code_macro && line_closes_prelude_block(line) {
            in_generated_code_macro = false;
        }
        previous_line = line;
    }
    violations
}

fn line_opens_generated_code_macro(line: &str) -> bool {
    line.contains("\u{00a4}rust_chunk(") || line.contains("\u{00a4}rust_file(")
}

fn line_opens_table_macro(line: &str) -> bool {
    line.contains("\u{00a4}table(")
}

fn line_closes_prelude_block(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "\u{00a4}])" || trimmed == "\u{00a4}})"
}

fn lint_raw_wvb_tables(file: &Path, text: &str) -> Vec<LintViolation> {
    if !is_wvb_file(file) || is_prelude_file(file) {
        return Vec::new();
    }

    let mut violations = Vec::new();
    let mut in_table_macro = false;
    let mut in_generated_code_macro = false;
    for (idx, line) in text.lines().enumerate() {
        if line_opens_generated_code_macro(line) {
            in_generated_code_macro = true;
        }
        if line_opens_table_macro(line) {
            in_table_macro = true;
        }

        if line == "|===" && !in_table_macro && !in_generated_code_macro {
            violations.push(LintViolation {
                file: file.to_path_buf(),
                line: idx + 1,
                rule: LintRule::RawWvbTable,
                message: "raw AsciiDoc table fence outside `\u{00a4}table(...)` in `.wvb` source".to_string(),
                hint: Some("wrap table source in `\u{00a4}table(adoc, \u{00a4}[ ... \u{00a4}])` or `\u{00a4}table(adoc, \u{00a4}{ ... \u{00a4}})`".to_string()),
            });
        }

        if in_table_macro && line_closes_prelude_block(line) {
            in_table_macro = false;
        }
        if in_generated_code_macro && line_closes_prelude_block(line) {
            in_generated_code_macro = false;
        }
    }
    violations
}

pub fn lint_paths(
    paths: &[PathBuf],
    rule_filter: Option<LintRule>,
) -> Result<Vec<LintViolation>, String> {
    let mut source_files = Vec::new();
    let syntaxes = load_lint_syntaxes();
    if paths.is_empty() {
        collect_literate_files(Path::new("."), &mut source_files).map_err(|e| e.to_string())?;
    } else {
        for path in paths {
            collect_literate_files(path, &mut source_files).map_err(|e| e.to_string())?;
        }
    }
    source_files.sort();
    source_files.dedup();

    let mut violations = Vec::new();
    for file in source_files {
        let text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let file_syntaxes = lint_syntaxes_for_file(&file, &syntaxes);
        if rule_filter.is_none() || rule_filter == Some(LintRule::ChunkBodyOutsideFence) {
            violations.extend(lint_chunk_body_outside_fence(&file, &text, &file_syntaxes));
        }
        if rule_filter.is_none() || rule_filter == Some(LintRule::UnterminatedChunkDefinition) {
            violations.extend(lint_unterminated_chunk_definition(&file, &text, &file_syntaxes));
        }
        if rule_filter.is_none() || rule_filter == Some(LintRule::RawWvbLink) {
            violations.extend(lint_raw_wvb_links(&file, &text));
        }
        if rule_filter.is_none() || rule_filter == Some(LintRule::RawWvbSourceBlock) {
            violations.extend(lint_raw_wvb_source_blocks(&file, &text));
        }
        if rule_filter.is_none() || rule_filter == Some(LintRule::RawWvbTable) {
            violations.extend(lint_raw_wvb_tables(&file, &text));
        }
    }
    Ok(violations)
}
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
#[cfg(test)]
mod tests;

