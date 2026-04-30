// weaveback-api/src/lint/rules.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use super::config::{lint_syntaxes_for_file, load_lint_syntaxes};
use super::fs_scan::{collect_literate_files, is_prelude_file, is_wvb_file};

pub(in crate::lint) fn parse_chunk_definition_name(line: &str, syntaxes: &[&NowebSyntax]) -> Option<String> {
    for syntax in syntaxes {
        if let Some(def_match) = syntax.parse_definition_line(line) {
            return Some(def_match.base_name);
        }
    }
    None
}

pub(in crate::lint) fn parse_chunk_definition(line: &str, syntaxes: &[&NowebSyntax]) -> Option<(String, usize)> {
    for (idx, syntax) in syntaxes.iter().enumerate() {
        if let Some(def_match) = syntax.parse_definition_line(line) {
            return Some((def_match.base_name, idx));
        }
    }
    None
}

pub(in crate::lint) fn is_adoc_fence_delimiter(line: &str) -> bool {
    matches!(line.trim(), "----" | "....")
}

pub(in crate::lint) fn starts_wvb_code_block(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('\u{00a4}') && trimmed.contains("code_block(")
}

pub(in crate::lint) fn opens_wvb_block_arg(line: &str) -> bool {
    line.contains("\u{00a4}[") || line.contains("\u{00a4}{")
}

pub(in crate::lint) fn closes_wvb_block_arg(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "\u{00a4}])"
        || trimmed == "\u{00a4}})"
        || trimmed == "\u{00a4}])\u{00a4})"
        || trimmed == "\u{00a4}})\u{00a4})"
}

pub(in crate::lint) fn lint_chunk_body_outside_fence(
    file: &Path,
    text: &str,
    syntaxes: &[&NowebSyntax],
) -> Vec<LintViolation> {
    let mut in_fence = false;
    let is_wvb = file.extension().and_then(|ext| ext.to_str()) == Some("wvb");
    let mut in_wvb_code_block = false;
    let mut violations = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if is_wvb {
            if !in_wvb_code_block && starts_wvb_code_block(trimmed) && opens_wvb_block_arg(trimmed) {
                in_wvb_code_block = true;
            } else if in_wvb_code_block && closes_wvb_block_arg(trimmed) {
                in_wvb_code_block = false;
                continue;
            }
        }
        if is_adoc_fence_delimiter(trimmed) {
            in_fence = !in_fence;
            continue;
        }
        if let Some(chunk_name) =
            parse_chunk_definition_name(trimmed, syntaxes).filter(|_| !in_fence && !in_wvb_code_block)
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

pub(in crate::lint) fn lint_unterminated_chunk_definition(
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

pub(in crate::lint) fn lint_raw_wvb_links(file: &Path, text: &str) -> Vec<LintViolation> {
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

pub(in crate::lint) fn lint_raw_wvb_source_blocks(file: &Path, text: &str) -> Vec<LintViolation> {
    if !is_wvb_file(file) || is_prelude_file(file) {
        return Vec::new();
    }

    let mut violations = Vec::new();
    let mut in_generated_code_macro = false;
    let mut in_wvb_code_block = false;
    let mut previous_line = "";
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if !in_wvb_code_block && starts_wvb_code_block(trimmed) && opens_wvb_block_arg(trimmed) {
            in_wvb_code_block = true;
        } else if in_wvb_code_block && closes_wvb_block_arg(trimmed) {
            in_wvb_code_block = false;
        }

        if line_opens_generated_code_macro(line) {
            in_generated_code_macro = true;
        }

        let is_raw_source =
            line.starts_with("[source") || line.starts_with("[plantuml") || line.starts_with("[d2");
        let is_prelude_definition_body = previous_line.contains("\u{00a4}redef(");
        if is_raw_source && !in_generated_code_macro && !in_wvb_code_block && !is_prelude_definition_body {
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

pub(in crate::lint) fn line_opens_generated_code_macro(line: &str) -> bool {
    line.contains("\u{00a4}rust_chunk(") || line.contains("\u{00a4}rust_file(")
}

pub(in crate::lint) fn line_opens_table_macro(line: &str) -> bool {
    line.contains("\u{00a4}table(")
}

pub(in crate::lint) fn line_closes_prelude_block(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "\u{00a4}])" || trimmed == "\u{00a4}})"
}

pub(in crate::lint) fn lint_raw_wvb_tables(file: &Path, text: &str) -> Vec<LintViolation> {
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

pub(in crate::lint) fn lint_paths(
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

