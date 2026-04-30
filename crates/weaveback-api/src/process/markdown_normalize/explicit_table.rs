// weaveback-api/src/process/markdown_normalize/explicit_table.rs
// I'd Really Rather You Didn't edit this generated file.

use super::{is_asciidoc_ext, is_markdown_ext};
use super::adoc_table::normalize_adoc_tables_for_markdown;
use super::markdown_table::normalize_markdown_table_for_asciidoc;

pub(in crate::process::markdown_normalize) fn render_explicit_table_block(expanded_ext: Option<&str>, format: &str, body: &str) -> String {
    let format = format.trim().to_ascii_lowercase();
    let body = body.trim_matches('\n');
    if is_markdown_ext(expanded_ext) {
        match format.as_str() {
            "adoc" | "asciidoc" => normalize_adoc_tables_for_markdown(body),
            "md" | "markdown" | "html" => body.to_string(),
            _ => body.to_string(),
        }
    } else if is_asciidoc_ext(expanded_ext) {
        match format.as_str() {
            "md" | "markdown" => normalize_markdown_table_for_asciidoc(body),
            "html" => format!("++++\n{body}\n++++"),
            "adoc" | "asciidoc" => body.to_string(),
            _ => body.to_string(),
        }
    } else {
        body.to_string()
    }
}

pub(in crate::process::markdown_normalize) fn normalize_explicit_table_blocks(expanded_ext: Option<&str>, input: &str) -> String {
    const TABLE_START_PREFIX: &str = concat!("<", "!-- weaveback-table:");
    const TABLE_END: &str = concat!("<", "!-- /weaveback-table -->");

    let lines: Vec<&str> = input.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut idx = 0;
    let mut in_fence: Option<&str> = None;

    while idx < lines.len() {
        let trimmed = lines[idx].trim();
        if trimmed.starts_with("```") {
            in_fence = if in_fence == Some("```") { None } else { Some("```") };
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }
        if trimmed.starts_with("~~~") {
            in_fence = if in_fence == Some("~~~") { None } else { Some("~~~") };
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }
        if trimmed == "----" {
            in_fence = if in_fence == Some("----") { None } else { Some("----") };
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }
        if trimmed == "...." {
            in_fence = if in_fence == Some("....") { None } else { Some("....") };
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }
        if in_fence.is_some() {
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }

        if !trimmed.starts_with(TABLE_START_PREFIX) || !trimmed.ends_with("-->") {
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }

        let format = trimmed
            .trim_start_matches(TABLE_START_PREFIX)
            .trim_end_matches("-->")
            .trim();
        let body_start = idx + 1;
        idx = body_start;
        while idx < lines.len() && lines[idx].trim() != TABLE_END {
            idx += 1;
        }
        if idx == lines.len() {
            out.extend(lines[body_start - 1..].iter().map(|line| (*line).to_string()));
            break;
        }

        out.push(render_explicit_table_block(
            expanded_ext,
            format,
            &lines[body_start..idx].join("\n"),
        ));
        idx += 1;
    }

    let mut rendered = out.join("\n");
    if input.ends_with('\n') {
        rendered.push('\n');
    }
    rendered
}

