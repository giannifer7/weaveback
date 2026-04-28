// weaveback-api/src/coverage/tests_coverage/summary_output.rs
// I'd Really Rather You Didn't edit this generated file.

#[test]
fn print_coverage_summary_shows_attributed_vs_total() {
    let summary = json!({
        "attributed_records": 80,
        "line_records": 100,
        "sources": [],
        "unattributed_records": 0,
        "unattributed_files": []
    });
    let mut out = Vec::new();
    print_coverage_summary_to_writer(&summary, 10, 5, false, &mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("80 attributed"), "got: {s}");
    assert!(s.contains("100 total"), "got: {s}");
}

#[test]
fn print_coverage_summary_prints_source_stats() {
    let summary = json!({
        "attributed_records": 4,
        "line_records": 10,
        "unattributed_records": 0,
        "unattributed_files": [],
        "sources": [{
            "src_file": "src/foo.adoc",
            "covered_lines": 6,
            "missed_lines": 4,
            "total_lines": 10,
            "sections": [{
                "source_section_breadcrumb": ["Module", "Parser"],
                "covered_lines": 3,
                "missed_lines": 2,
                "total_lines": 5
            }]
        }]
    });
    let mut out = Vec::new();
    print_coverage_summary_to_writer(&summary, 10, 5, false, &mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("src/foo.adoc"), "got: {s}");
    assert!(s.contains("Module / Parser"), "got: {s}");
    assert!(s.contains("60.0%"), "got: {s}");
}

#[test]
fn print_coverage_summary_reports_unattributed_files() {
    let summary = json!({
        "attributed_records": 0,
        "line_records": 5,
        "unattributed_records": 5,
        "sources": [],
        "unattributed_files": [{
            "generated_file": "gen/out.rs",
            "covered_lines": 0,
            "missed_lines": 5,
            "total_lines": 5,
            "has_noweb_entries": false,
            "unmapped_ranges": []
        }]
    });
    let mut out = Vec::new();
    print_coverage_summary_to_writer(&summary, 10, 5, false, &mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("Unattributed"), "got: {s}");
    assert!(s.contains("gen/out.rs"), "got: {s}");
    assert!(s.contains("no noweb mapping"), "got: {s}");
}

// ── explain_unattributed_file ──────────────────────────────────────────

#[test]
fn explain_unattributed_file_emits_ranges() {
    let file = json!({
        "generated_file": "gen/out.rs",
        "unmapped_ranges": [{"start": 10, "end": 12, "missed_count": 2}]
    });
    let mut out = Vec::new();
    explain_unattributed_file(&file, false, &mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("10-12"), "got: {s}");
    assert!(s.contains("2 missed"), "got: {s}");
}

#[test]
fn explain_unattributed_file_skips_empty_ranges() {
    let file = json!({"unmapped_ranges": []});
    let mut out = Vec::new();
    explain_unattributed_file(&file, false, &mut out).unwrap();
    assert!(out.is_empty());
}

// ── build_coverage_summary_view ───────────────────────────────────────

#[test]
fn build_coverage_summary_view_truncates_sources_and_sections() {
    let summary = json!({
        "attributed_records": 10,
        "unattributed_records": 0,
        "line_records": 10,
        "unattributed_files": [],
        "sources": [
            {"src_file": "a.adoc", "sections": [{"s": 1}, {"s": 2}, {"s": 3}]},
            {"src_file": "b.adoc", "sections": [{"s": 4}]},
            {"src_file": "c.adoc", "sections": []},
        ]
    });
    let view = build_coverage_summary_view(&summary, 2, 1);
    let sv = &view["summary_view"];
    let srcs = sv["sources"].as_array().unwrap();
    assert_eq!(srcs.len(), 2, "top_sources=2 → 2 sources");
    // First source should have at most 1 section
    let secs = srcs[0]["sections"].as_array().unwrap();
    assert_eq!(secs.len(), 1, "top_sections=1 → 1 section");
}

#[test]
fn build_coverage_summary_view_preserves_metadata_fields() {
    let summary = json!({
        "attributed_records": 7,
        "line_records": 10,
        "unattributed_records": 3,
        "unattributed_files": [],
        "sources": []
    });
    let view = build_coverage_summary_view(&summary, 5, 5);
    let sv = &view["summary_view"];
    assert_eq!(sv["attributed_records"], 7);
    assert_eq!(sv["line_records"], 10);
    assert_eq!(sv["unattributed_records"], 3);
}

// ── emit_cargo_summary_message ────────────────────────────────────────
#[test]
fn build_location_attribution_summary_groups_by_source_and_section() {
    let records = vec![
        json!({
            "location": "gen/a.rs:1:1",
            "ok": true,
            "trace": {
                "src_file": "src/foo.adoc",
                "chunk": "alpha",
                "source_section_breadcrumb": ["Root", "Foo"]
            }
        }),
        json!({
            "location": "gen/a.rs:2:1",
            "ok": true,
            "trace": {
                "src_file": "src/foo.adoc",
                "chunk": "alpha",
                "source_section_breadcrumb": ["Root", "Foo"]
            }
        }),
        json!({
            "location": "gen/b.rs:1:1",
            "ok": true,
            "trace": {
                "src_file": "src/bar.adoc",
                "chunk": "beta",
                "source_section_breadcrumb": ["Root", "Bar"]
            }
        }),
    ];
    let summary = build_location_attribution_summary(&records);
    let sources = summary["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 2);
    let foo = sources.iter().find(|s| s["src_file"] == "src/foo.adoc").unwrap();
    assert_eq!(foo["count"], 2);
}

#[test]
fn build_location_attribution_summary_returns_empty_for_no_records() {
    let summary = build_location_attribution_summary(&[]);
    assert_eq!(summary["count"], 0);
    assert!(summary["sources"].as_array().unwrap().is_empty());
}

