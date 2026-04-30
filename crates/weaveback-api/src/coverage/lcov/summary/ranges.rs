// weaveback-api/src/coverage/lcov/summary/ranges.rs
// I'd Really Rather You Didn't edit this generated file.

use super::super::*;

/// Group a `generated_lines` slice into consecutive ranges.
/// Returns a JSON array of `{start, end, missed_count}` objects so the
/// result can be embedded directly in the summary JSON and consumed by
/// both agents (via JSON output) and humans (via `--summary`).
pub(in crate::coverage) fn compute_unmapped_ranges(generated_lines: &[serde_json::Value]) -> serde_json::Value {
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

