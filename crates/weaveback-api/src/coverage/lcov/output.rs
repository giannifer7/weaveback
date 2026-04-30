// weaveback-api/src/coverage/lcov/output.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

/// Print unmapped line ranges for one unattributed file.
/// Reads `unmapped_ranges` from the pre-computed JSON field.
/// When `show_content` is true and the file is readable, prints source lines.
pub(in crate::coverage) fn explain_unattributed_file(
    file: &serde_json::Value,
    show_content: bool,
    out: &mut impl Write,
) -> std::io::Result<()> {
    let ranges = match file["unmapped_ranges"].as_array() {
        Some(r) if !r.is_empty() => r,
        _ => return Ok(()),
    };
    writeln!(out, "    unmapped ranges:")?;

    let file_lines: Option<Vec<String>> = if show_content {
        let path = file["generated_file"].as_str().unwrap_or("");
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.lines().map(str::to_owned).collect())
    } else {
        None
    };

    for range in ranges {
        let start  = range["start"].as_u64().unwrap_or(0);
        let end    = range["end"].as_u64().unwrap_or(0);
        let missed = range["missed_count"].as_u64().unwrap_or(0);
        let count  = end - start + 1;
        writeln!(out, "      {start}-{end} ({count} line(s), {missed} missed):")?;
        if let Some(lines) = &file_lines {
            let lo = (start as usize).saturating_sub(1);
            let hi = (end as usize).min(lines.len());
            for (i, line) in lines[lo..hi].iter().enumerate() {
                let ln = start + i as u64;
                writeln!(out, "        {ln}: {line}")?;
            }
        }
    }
    Ok(())
}

pub(in crate::coverage) fn print_coverage_summary_to_writer(
    summary: &serde_json::Value,
    top_sources: usize,
    top_sections: usize,
    explain_unattributed: bool,
    mut out: impl Write,
) -> std::io::Result<()> {
    writeln!(
        out,
        "Coverage by source: {} attributed / {} total line records",
        summary["attributed_records"].as_u64().unwrap_or(0),
        summary["line_records"].as_u64().unwrap_or(0)
    )?;

    if let Some(sources) = summary["sources"].as_array() {
        for source in sources.iter().take(top_sources) {
            let src_file = source["src_file"].as_str().unwrap_or("<unknown>");
            let covered = source["covered_lines"].as_u64().unwrap_or(0);
            let missed = source["missed_lines"].as_u64().unwrap_or(0);
            let total = source["total_lines"].as_u64().unwrap_or(0);
            let pct = if total == 0 {
                0.0
            } else {
                100.0 * covered as f64 / total as f64
            };
            writeln!(out, "{src_file}: {covered}/{total} covered ({pct:.1}%%), {missed} missed")?;
            if let Some(sections) = source["sections"].as_array() {
                for section in sections.iter().take(top_sections) {
                    let breadcrumb = section["source_section_breadcrumb"]
                        .as_array()
                        .map(|parts| {
                            parts
                                .iter()
                                .filter_map(|part| part.as_str())
                                .collect::<Vec<_>>()
                                .join(" / ")
                        })
                        .unwrap_or_else(|| "<unknown>".to_string());
                    let covered = section["covered_lines"].as_u64().unwrap_or(0);
                    let missed = section["missed_lines"].as_u64().unwrap_or(0);
                    let total = section["total_lines"].as_u64().unwrap_or(0);
                    let pct = if total == 0 {
                        0.0
                    } else {
                        100.0 * covered as f64 / total as f64
                    };
                    writeln!(
                        out,
                        "  {breadcrumb}: {covered}/{total} covered ({pct:.1}%%), {missed} missed"
                    )?;
                }
            }
        }
    }

    let unattributed = summary["unattributed_records"].as_u64().unwrap_or(0);
    if unattributed > 0 {
        writeln!(out, "Unattributed line records: {unattributed}")?;
        if let Some(files) = summary["unattributed_files"].as_array() {
            for file in files.iter().take(top_sources) {
                let generated_file = file["generated_file"].as_str().unwrap_or("<unknown>");
                let covered = file["covered_lines"].as_u64().unwrap_or(0);
                let missed = file["missed_lines"].as_u64().unwrap_or(0);
                let total = file["total_lines"].as_u64().unwrap_or(0);
                let pct = if total == 0 {
                    0.0
                } else {
                    100.0 * covered as f64 / total as f64
                };
                writeln!(
                    out,
                    "  {generated_file}: {covered}/{total} covered ({pct:.1}%%), {missed} missed"
                )?;
                if file["has_noweb_entries"].as_bool().unwrap_or(false) {
                    let start = file["mapped_line_start"].as_u64().unwrap_or(0);
                    let end = file["mapped_line_end"].as_u64().unwrap_or(0);
                    writeln!(out, "    partial mapping: mapped lines {start}-{end}")?;
                } else {
                    writeln!(out, "    no noweb mapping recorded for this file")?;
                }
                if explain_unattributed {
                    explain_unattributed_file(file, true, &mut out)?;
                }
            }
        }
    }
    Ok(())
}

pub fn build_coverage_summary_view(
    summary: &serde_json::Value,
    top_sources: usize,
    top_sections: usize,
) -> serde_json::Value {
    let mut value = summary.clone();
    let top_sources_value = summary["sources"]
        .as_array()
        .map(|sources| {
            serde_json::Value::Array(
                sources
                    .iter()
                    .take(top_sources)
                    .map(|source| {
                        let mut source = source.clone();
                        if let Some(obj) = source.as_object_mut()
                            && let Some(sections) =
                                obj.get("sections").and_then(|v| v.as_array()).cloned()
                        {
                            obj.insert(
                                "sections".to_string(),
                                serde_json::Value::Array(
                                    sections.into_iter().take(top_sections).collect(),
                                ),
                            );
                        }
                        source
                    })
                    .collect(),
            )
        })
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));

    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "summary_view".to_string(),
            json!({
                "top_sources": top_sources,
                "top_sections": top_sections,
                "sources": top_sources_value,
                "unattributed_records": summary["unattributed_records"].clone(),
                "unattributed_files": summary["unattributed_files"]
                    .as_array()
                    .map(|files| serde_json::Value::Array(files.iter().take(top_sources).cloned().collect()))
                    .unwrap_or_else(|| serde_json::Value::Array(Vec::new())),
                "line_records": summary["line_records"].clone(),
                "attributed_records": summary["attributed_records"].clone(),
            }),
        );
    }
    value
}

