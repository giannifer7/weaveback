// weaveback-api/src/coverage/cargo.rs
// I'd Really Rather You Didn't edit this generated file.

#[derive(Debug, serde::Deserialize)]
struct CargoMessageEnvelope {
    reason: String,
    message: Option<CargoDiagnostic>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CargoDiagnostic {
    pub spans: Vec<CargoDiagnosticSpan>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CargoDiagnosticSpan {
    pub file_name: String,
    pub line_start: u32,
    pub column_start: u32,
    pub is_primary: bool,
}

pub fn collect_cargo_attributions(
    diagnostic: &CargoDiagnostic,
    db: Option<&weaveback_tangle::db::WeavebackDb>,
    project_root: &Path,
    resolver: &PathResolver,
    eval_config: &EvalConfig,
) -> Vec<serde_json::Value> {
    let Some(db) = db else {
        return Vec::new();
    };
    let mut records = Vec::new();
    let mut seen = HashSet::new();

    for span in diagnostic.spans.iter().filter(|span| span.is_primary) {
        let Some(trace) = trace_generated_location(
            &span.file_name,
            span.line_start,
            span.column_start,
            db,
            project_root,
            resolver,
            eval_config,
        ) else {
            continue;
        };

        let dedupe_key = serde_json::to_string(&trace).unwrap_or_default();
        if seen.insert(dedupe_key) {
            records.push(trace);
        }
    }

    records
}

pub fn collect_cargo_span_attributions(
    diagnostic: &CargoDiagnostic,
    db: Option<&weaveback_tangle::db::WeavebackDb>,
    project_root: &Path,
    resolver: &PathResolver,
    eval_config: &EvalConfig,
) -> Vec<serde_json::Value> {
    let Some(db) = db else {
        return Vec::new();
    };
    let mut records = Vec::new();
    let mut seen = HashSet::new();

    for span in &diagnostic.spans {
        let Some(trace) = trace_generated_location(
            &span.file_name,
            span.line_start,
            span.column_start,
            db,
            project_root,
            resolver,
            eval_config,
        ) else {
            continue;
        };

        let record = json!({
            "generated_file": span.file_name,
            "generated_line": span.line_start,
            "generated_col": span.column_start,
            "is_primary": span.is_primary,
            "trace": trace,
        });
        let dedupe_key = serde_json::to_string(&record).unwrap_or_default();
        if seen.insert(dedupe_key) {
            records.push(record);
        }
    }

    records
}

fn trace_generated_location(
    file_name: &str,
    line: u32,
    col: u32,
    db: &weaveback_tangle::db::WeavebackDb,
    project_root: &Path,
    resolver: &PathResolver,
    eval_config: &EvalConfig,
) -> Option<serde_json::Value> {
    if let Ok(Some(value)) =
        lookup::perform_trace(file_name, line, col, db, resolver, eval_config.clone())
    {
        return Some(value);
    }

    let file_path = Path::new(file_name);
    let rel = file_path
        .strip_prefix(project_root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))?;
    lookup::perform_trace(&rel, line, col, db, resolver, eval_config.clone())
        .ok()
        .flatten()
}

pub fn build_cargo_attribution_summary(
    span_attributions: &[serde_json::Value],
) -> serde_json::Value {
    #[derive(Default)]
    struct SectionSummary {
        count: usize,
        chunks: std::collections::BTreeSet<String>,
        generated_spans: Vec<serde_json::Value>,
        prose: Option<String>,
        range: Option<serde_json::Value>,
        breadcrumb: Vec<String>,
    }

    #[derive(Default)]
    struct SourceSummary {
        count: usize,
        chunks: std::collections::BTreeSet<String>,
        sections: std::collections::BTreeMap<String, SectionSummary>,
    }

    let mut grouped: std::collections::BTreeMap<String, SourceSummary> =
        std::collections::BTreeMap::new();

    for record in span_attributions {
        let Some(trace) = record.get("trace") else {
            continue;
        };
        let Some(src_file) = trace
            .get("src_file")
            .and_then(|v| v.as_str())
            .or_else(|| trace.get("expanded_file").and_then(|v| v.as_str()))
        else {
            continue;
        };
        let chunk = trace
            .get("chunk")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let breadcrumb = trace
            .get("source_section_breadcrumb")
            .and_then(|v| v.as_array())
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| part.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let section_key = if breadcrumb.is_empty() {
            "<unknown>".to_string()
        } else {
            breadcrumb.join(" / ")
        };
        let generated_span = json!({
            "generated_file": record.get("generated_file").cloned().unwrap_or(serde_json::Value::Null),
            "generated_line": record.get("generated_line").cloned().unwrap_or(serde_json::Value::Null),
            "generated_col": record.get("generated_col").cloned().unwrap_or(serde_json::Value::Null),
            "is_primary": record.get("is_primary").cloned().unwrap_or(serde_json::Value::Bool(false)),
            "chunk": if chunk.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(chunk.clone()) },
        });
        let entry = grouped
            .entry(src_file.to_string())
            .or_default();
        entry.count += 1;
        if !chunk.is_empty() {
            entry.chunks.insert(chunk.clone());
        }
        let section = entry.sections.entry(section_key).or_default();
        section.count += 1;
        if !chunk.is_empty() {
            section.chunks.insert(chunk);
        }
        section.generated_spans.push(generated_span);
        if section.prose.is_none() {
            section.prose = trace
                .get("source_section_prose")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned);
        }
        if section.range.is_none() {
            section.range = trace.get("source_section_range").cloned();
        }
        if section.breadcrumb.is_empty() {
            section.breadcrumb = breadcrumb;
        }
    }

    json!({
        "count": span_attributions.len(),
        "sources": grouped
            .into_iter()
            .map(|(src_file, summary)| json!({
                "src_file": src_file,
                "count": summary.count,
                "chunks": summary.chunks.into_iter().collect::<Vec<_>>(),
                "sections": summary
                    .sections
                    .into_values()
                    .map(|section| json!({
                        "count": section.count,
                        "chunks": section.chunks.into_iter().collect::<Vec<_>>(),
                        "generated_spans": section.generated_spans,
                        "source_section_breadcrumb": section.breadcrumb,
                        "source_section_range": section.range.unwrap_or(serde_json::Value::Null),
                        "source_section_prose": section.prose.unwrap_or_default(),
                    }))
                    .collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
    })
}

pub fn build_location_attribution_summary(records: &[serde_json::Value]) -> serde_json::Value {
    #[derive(Default)]
    struct SectionSummary {
        count: usize,
        chunks: std::collections::BTreeSet<String>,
        locations: Vec<String>,
        prose: Option<String>,
        range: Option<serde_json::Value>,
        breadcrumb: Vec<String>,
    }

    #[derive(Default)]
    struct SourceSummary {
        count: usize,
        chunks: std::collections::BTreeSet<String>,
        sections: std::collections::BTreeMap<String, SectionSummary>,
    }

    let mut grouped: std::collections::BTreeMap<String, SourceSummary> =
        std::collections::BTreeMap::new();

    for record in records.iter().filter(|record| record["ok"].as_bool() == Some(true)) {
        let Some(trace) = record.get("trace") else {
            continue;
        };
        let Some(src_file) = trace
            .get("src_file")
            .and_then(|v| v.as_str())
            .or_else(|| trace.get("expanded_file").and_then(|v| v.as_str()))
        else {
            continue;
        };
        let chunk = trace
            .get("chunk")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let breadcrumb = trace
            .get("source_section_breadcrumb")
            .and_then(|v| v.as_array())
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| part.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let section_key = if breadcrumb.is_empty() {
            "<unknown>".to_string()
        } else {
            breadcrumb.join(" / ")
        };

        let source = grouped.entry(src_file.to_string()).or_default();
        source.count += 1;
        if !chunk.is_empty() {
            source.chunks.insert(chunk.clone());
        }

        let section = source.sections.entry(section_key).or_default();
        section.count += 1;
        if !chunk.is_empty() {
            section.chunks.insert(chunk);
        }
        if let Some(location) = record.get("location").and_then(|v| v.as_str()) {
            section.locations.push(location.to_string());
        }
        if section.prose.is_none() {
            section.prose = trace
                .get("source_section_prose")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned);
        }
        if section.range.is_none() {
            section.range = trace.get("source_section_range").cloned();
        }
        if section.breadcrumb.is_empty() {
            section.breadcrumb = breadcrumb;
        }
    }

    json!({
        "count": records.iter().filter(|record| record["ok"].as_bool() == Some(true)).count(),
        "sources": grouped
            .into_iter()
            .map(|(src_file, summary)| {
                let mut sections = summary
                    .sections
                    .into_values()
                    .map(|section| json!({
                        "count": section.count,
                        "chunks": section.chunks.into_iter().collect::<Vec<_>>(),
                        "locations": section.locations,
                        "source_section_breadcrumb": section.breadcrumb,
                        "source_section_range": section.range.unwrap_or(serde_json::Value::Null),
                        "source_section_prose": section.prose.unwrap_or_default(),
                    }))
                    .collect::<Vec<_>>();
                sections.sort_by(|a, b| {
                    let ac = a["count"].as_u64().unwrap_or(0);
                    let bc = b["count"].as_u64().unwrap_or(0);
                    bc.cmp(&ac)
                });
                json!({
                    "src_file": src_file,
                    "count": summary.count,
                    "chunks": summary.chunks.into_iter().collect::<Vec<_>>(),
                    "sections": sections,
                })
            })
            .collect::<Vec<_>>(),
    })
}

pub fn emit_augmented_cargo_message(
    original_line: &str,
    attributions: Vec<serde_json::Value>,
    span_attributions: Vec<serde_json::Value>,
    mut out: impl Write,
) -> std::io::Result<()> {
    let mut value: serde_json::Value = match serde_json::from_str(original_line) {
        Ok(value) => value,
        Err(_) => {
            writeln!(out, "{original_line}")?;
            return Ok(());
        }
    };
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "weaveback_attributions".to_string(),
            serde_json::Value::Array(attributions),
        );
        obj.insert(
            "weaveback_span_attributions".to_string(),
            serde_json::Value::Array(span_attributions.clone()),
        );
        obj.insert(
            "weaveback_source_summary".to_string(),
            build_cargo_attribution_summary(&span_attributions),
        );
    }
    serde_json::to_writer(&mut out, &value)?;
    writeln!(out)?;
    Ok(())
}

pub fn emit_cargo_summary_message(
    compiler_message_count: usize,
    span_attributions: &[serde_json::Value],
    mut out: impl Write,
) -> std::io::Result<()> {
    serde_json::to_writer(
        &mut out,
        &json!({
            "reason": "weaveback-summary",
            "compiler_message_count": compiler_message_count,
            "generated_span_count": span_attributions.len(),
            "weaveback_source_summary": build_cargo_attribution_summary(span_attributions),
        }),
    )?;
    writeln!(out)?;
    Ok(())
}

