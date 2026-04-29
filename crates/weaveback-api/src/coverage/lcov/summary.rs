// weaveback-api/src/coverage/lcov/summary.rs
// I'd Really Rather You Didn't edit this generated file.

pub fn build_coverage_summary(
    records: &[(String, u32, u64)],
    db: &weaveback_tangle::db::WeavebackDb,
    project_root: &Path,
    resolver: &PathResolver,
) -> serde_json::Value {
    #[derive(Default)]
    struct SectionSummary {
        total_lines: usize,
        covered_lines: usize,
        missed_lines: usize,
        chunks: std::collections::BTreeSet<String>,
        generated_lines: Vec<serde_json::Value>,
        prose: Option<String>,
        range: Option<serde_json::Value>,
        breadcrumb: Vec<String>,
    }

    #[derive(Default)]
    struct SourceSummary {
        total_lines: usize,
        covered_lines: usize,
        missed_lines: usize,
        chunks: std::collections::BTreeSet<String>,
        sections: std::collections::BTreeMap<String, SectionSummary>,
    }

    #[derive(Default)]
    struct UnattributedSummary {
        total_lines: usize,
        covered_lines: usize,
        missed_lines: usize,
        has_noweb_entries: bool,
        mapped_line_start: Option<u32>,
        mapped_line_end: Option<u32>,
        generated_lines: Vec<serde_json::Value>,
    }

    let mut grouped: std::collections::BTreeMap<String, SourceSummary> =
        std::collections::BTreeMap::new();
    let mut unattributed_grouped: std::collections::BTreeMap<String, UnattributedSummary> =
        std::collections::BTreeMap::new();
    let mut unattributed = Vec::new();
    let mut attributed_count = 0usize;
    let mut noweb_cache: std::collections::HashMap<
        String,
        std::collections::HashMap<u32, weaveback_tangle::db::NowebMapEntry>,
    > = std::collections::HashMap::new();
    let mut source_cache: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut section_cache: std::collections::HashMap<String, Vec<(u32, u32, serde_json::Value)>> =
        std::collections::HashMap::new();

    for (file_name, line_no, hit_count) in records {
        let noweb_map = if let Some(entries) = noweb_cache.get(file_name) {
            entries
        } else {
            let loaded = find_noweb_entries_for_generated_file(db, file_name, project_root)
                .unwrap_or_default()
                .into_iter()
                .collect::<std::collections::HashMap<_, _>>();
            noweb_cache.entry(file_name.clone()).or_insert(loaded)
        };

        let Some(entry) = line_no
            .checked_sub(1)
            .and_then(|line_0| noweb_map.get(&line_0))
        else {
            let covered = *hit_count > 0;
            let mapped_line_start = noweb_map.keys().min().copied().map(|line_0| line_0 + 1);
            let mapped_line_end = noweb_map.keys().max().copied().map(|line_0| line_0 + 1);
            let generated_line = json!({
                "generated_file": file_name,
                "generated_line": line_no,
                "hit_count": hit_count,
                "covered": covered,
                "has_noweb_entries": !noweb_map.is_empty(),
                "mapped_line_start": mapped_line_start,
                "mapped_line_end": mapped_line_end,
            });
            unattributed.push(generated_line.clone());
            let file = unattributed_grouped.entry(file_name.clone()).or_default();
            file.total_lines += 1;
            if covered {
                file.covered_lines += 1;
            } else {
                file.missed_lines += 1;
            }
            file.has_noweb_entries |= !noweb_map.is_empty();
            file.mapped_line_start = match (file.mapped_line_start, mapped_line_start) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (None, b) => b,
                (a, None) => a,
            };
            file.mapped_line_end = match (file.mapped_line_end, mapped_line_end) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (None, b) => b,
                (a, None) => a,
            };
            file.generated_lines.push(generated_line);
            continue;
        };

        let src_file = entry.src_file.clone();
        let src_line = (entry.src_line + 1) as u64;

        let context = if let Some(cached) = section_cache
            .get(&src_file)
            .and_then(|sections| {
                sections.iter().find_map(|(start, end, value)| {
                    if src_line >= *start as u64 && src_line <= *end as u64 {
                        Some(value.clone())
                    } else {
                        None
                    }
                })
            }) {
            cached
        } else {
            let src_content = if let Some(text) = source_cache.get(&src_file) {
                text.clone()
            } else {
                let Ok(text) = lookup::load_source_text(&src_file, db, resolver) else {
                    let covered = *hit_count > 0;
                    let mapped_line_start = noweb_map.keys().min().copied().map(|line_0| line_0 + 1);
                    let mapped_line_end = noweb_map.keys().max().copied().map(|line_0| line_0 + 1);
                    let generated_line = json!({
                        "generated_file": file_name,
                        "generated_line": line_no,
                        "hit_count": hit_count,
                        "covered": covered,
                        "has_noweb_entries": !noweb_map.is_empty(),
                        "mapped_line_start": mapped_line_start,
                        "mapped_line_end": mapped_line_end,
                    });
                    unattributed.push(generated_line.clone());
                    let file = unattributed_grouped.entry(file_name.clone()).or_default();
                    file.total_lines += 1;
                    if covered {
                        file.covered_lines += 1;
                    } else {
                        file.missed_lines += 1;
                    }
                    file.has_noweb_entries |= !noweb_map.is_empty();
                    file.mapped_line_start = match (file.mapped_line_start, mapped_line_start) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (None, b) => b,
                        (a, None) => a,
                    };
                    file.mapped_line_end = match (file.mapped_line_end, mapped_line_end) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        (None, b) => b,
                        (a, None) => a,
                    };
                    file.generated_lines.push(generated_line);
                    continue;
                };
                source_cache.insert(src_file.clone(), text.clone());
                text
            };
            let value = lookup::build_source_context_value(&src_content, src_line as usize);
            let start = value
                .get("source_section_range")
                .and_then(|v| v.get("start_line"))
                .and_then(|v| v.as_u64())
                .unwrap_or(src_line) as u32;
            let end = value
                .get("source_section_range")
                .and_then(|v| v.get("end_line"))
                .and_then(|v| v.as_u64())
                .unwrap_or(src_line) as u32;
            section_cache
                .entry(src_file.clone())
                .or_default()
                .push((start, end, value.clone()));
            value
        };

        attributed_count += 1;
        let mut trace = json!({
            "generated_file": file_name,
            "generated_line": line_no,
            "chunk": entry.chunk_name,
            "expanded_file": src_file,
            "expanded_line": src_line,
            "indent": entry.indent,
            "confidence": entry.confidence.as_str(),
        });
        if let (Some(trace_obj), Some(ctx_obj)) = (trace.as_object_mut(), context.as_object()) {
            trace_obj.extend(ctx_obj.clone());
        }

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
        let covered = *hit_count > 0;
        let chunk = trace
            .get("chunk")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let generated_line = json!({
            "generated_file": file_name,
            "generated_line": line_no,
            "hit_count": hit_count,
            "covered": covered,
            "chunk": if chunk.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(chunk.clone()) },
        });

        let source = grouped.entry(src_file).or_default();
        source.total_lines += 1;
        if covered {
            source.covered_lines += 1;
        } else {
            source.missed_lines += 1;
        }
        if !chunk.is_empty() {
            source.chunks.insert(chunk.clone());
        }

        let section = source.sections.entry(section_key).or_default();
        section.total_lines += 1;
        if covered {
            section.covered_lines += 1;
        } else {
            section.missed_lines += 1;
        }
        if !chunk.is_empty() {
            section.chunks.insert(chunk);
        }
        section.generated_lines.push(generated_line);
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

    let mut sources = grouped
        .into_iter()
        .map(|(src_file, source)| {
            let mut sections = source
                .sections
                .into_values()
                .map(|section| {
                    json!({
                        "source_section_breadcrumb": section.breadcrumb,
                        "source_section_range": section.range.unwrap_or(serde_json::Value::Null),
                        "source_section_prose": section.prose.unwrap_or_default(),
                        "total_lines": section.total_lines,
                        "covered_lines": section.covered_lines,
                        "missed_lines": section.missed_lines,
                        "chunks": section.chunks.into_iter().collect::<Vec<_>>(),
                        "generated_lines": section.generated_lines,
                    })
                })
                .collect::<Vec<_>>();
            sections.sort_by(|a, b| {
                let am = a["missed_lines"].as_u64().unwrap_or(0);
                let bm = b["missed_lines"].as_u64().unwrap_or(0);
                bm.cmp(&am).then_with(|| {
                    let an = a["source_section_breadcrumb"]
                        .as_array()
                        .map(|parts| {
                            parts
                                .iter()
                                .filter_map(|part| part.as_str())
                                .collect::<Vec<_>>()
                                .join(" / ")
                        })
                        .unwrap_or_default();
                    let bn = b["source_section_breadcrumb"]
                        .as_array()
                        .map(|parts| {
                            parts
                                .iter()
                                .filter_map(|part| part.as_str())
                                .collect::<Vec<_>>()
                                .join(" / ")
                        })
                        .unwrap_or_default();
                    an.cmp(&bn)
                })
            });

            json!({
                "src_file": src_file,
                "total_lines": source.total_lines,
                "covered_lines": source.covered_lines,
                "missed_lines": source.missed_lines,
                "chunks": source.chunks.into_iter().collect::<Vec<_>>(),
                "sections": sections,
            })
        })
        .collect::<Vec<_>>();
    sources.sort_by(|a, b| {
        let am = a["missed_lines"].as_u64().unwrap_or(0);
        let bm = b["missed_lines"].as_u64().unwrap_or(0);
        bm.cmp(&am).then_with(|| {
            let af = a["src_file"].as_str().unwrap_or_default();
            let bf = b["src_file"].as_str().unwrap_or_default();
            af.cmp(bf)
        })
    });

    let mut unattributed_files = unattributed_grouped
        .into_iter()
        .map(|(generated_file, summary)| {
            let unmapped_ranges = compute_unmapped_ranges(&summary.generated_lines);
            json!({
                "generated_file": generated_file,
                "total_lines": summary.total_lines,
                "covered_lines": summary.covered_lines,
                "missed_lines": summary.missed_lines,
                "has_noweb_entries": summary.has_noweb_entries,
                "mapped_line_start": summary.mapped_line_start,
                "mapped_line_end": summary.mapped_line_end,
                "unmapped_ranges": unmapped_ranges,
                "generated_lines": summary.generated_lines,
            })
        })
        .collect::<Vec<_>>();
    unattributed_files.sort_by(|a, b| {
        let am = a["missed_lines"].as_u64().unwrap_or(0);
        let bm = b["missed_lines"].as_u64().unwrap_or(0);
        bm.cmp(&am).then_with(|| {
            let af = a["generated_file"].as_str().unwrap_or_default();
            let bf = b["generated_file"].as_str().unwrap_or_default();
            af.cmp(bf)
        })
    });

    json!({
        "line_records": records.len(),
        "attributed_records": attributed_count,
        "unattributed_records": unattributed.len(),
        "sources": sources,
        "unattributed": unattributed,
        "unattributed_files": unattributed_files,
    })
}

fn find_noweb_entries_for_generated_file(
    db: &weaveback_tangle::db::WeavebackDb,
    file_name: &str,
    project_root: &Path,
) -> Option<Vec<(u32, weaveback_tangle::db::NowebMapEntry)>> {
    let mut candidates = Vec::new();
    candidates.push(file_name.to_string());
    let file_path = Path::new(file_name);
    if let Ok(rel) = file_path.strip_prefix(project_root) {
        let rel = rel.to_string_lossy().replace('\\', "/");
        if !candidates.contains(&rel) {
            candidates.push(rel);
        }
    }

    for candidate in candidates {
        if let Ok(entries) = db.get_noweb_entries_for_file(&candidate)
            && !entries.is_empty()
        {
            return Some(entries);
        }
        for suffix in distinctive_suffix_candidates(&candidate) {
            if let Ok(entries) = db.get_noweb_entries_for_file_by_suffix(&suffix)
                && !entries.is_empty()
            {
                return Some(entries);
            }
        }
    }

    None
}

/// Group a `generated_lines` slice into consecutive ranges.
/// Returns a JSON array of `{start, end, missed_count}` objects so the
/// result can be embedded directly in the summary JSON and consumed by
/// both agents (via JSON output) and humans (via `--summary`).
fn compute_unmapped_ranges(generated_lines: &[serde_json::Value]) -> serde_json::Value {
    let mut lines: Vec<(u64, bool)> = generated_lines
        .iter()
        .filter_map(|r| {
            let ln = r["generated_line"].as_u64()?;
            let hit = r["hit_count"].as_u64().unwrap_or(0) > 0;
            Some((ln, hit))
        })
        .collect();
    lines.sort_by_key(|(ln, _)| *ln);
    lines.dedup_by_key(|(ln, _)| *ln);

    let mut ranges: Vec<serde_json::Value> = Vec::new();
    for (ln, hit) in lines {
        let missed = !hit;
        if let Some(last) = ranges.last_mut() {
            let last_end = last["end"].as_u64().unwrap();
            if ln == last_end + 1 {
                *last.get_mut("end").unwrap() = json!(ln);
                if missed {
                    let mc = last["missed_count"].as_u64().unwrap_or(0);
                    *last.get_mut("missed_count").unwrap() = json!(mc + 1);
                }
                continue;
            }
        }
        ranges.push(json!({
            "start": ln,
            "end":   ln,
            "missed_count": if missed { 1u64 } else { 0u64 },
        }));
    }
    json!(ranges)
}

