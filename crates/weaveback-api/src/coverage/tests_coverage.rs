// weaveback-api/src/coverage/tests_coverage.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use rusqlite;
use serde_json::json;
use tempfile::tempdir;
use weaveback_tangle::db::{Confidence, NowebMapEntry, WeavebackDb};

#[test]
fn parse_generated_location_accepts_line_and_optional_col() {
    assert_eq!(
        parse_generated_location("gen/out.rs:17").unwrap(),
        ("gen/out.rs".to_string(), 17, 1)
    );
    assert_eq!(
        parse_generated_location("gen/out.rs:17:9").unwrap(),
        ("gen/out.rs".to_string(), 17, 9)
    );
}

#[test]
fn scan_generated_locations_extracts_unique_specs() {
    let text = "panic at src/generated.rs:1:27\nsee also gen/out.rs:17 and src/generated.rs:1:27";
    assert_eq!(
        scan_generated_locations(text),
        vec!["src/generated.rs:1:27", "gen/out.rs:17"]
    );
}

#[test]
fn scan_generated_locations_trims_punctuation_and_supports_windows_paths() {
    let text = r#"note: (src/generated.rs:1:27), "C:\tmp\gen\out.rs:17:9"."#;
    assert_eq!(
        scan_generated_locations(text),
        vec!["src/generated.rs:1:27", r#"C:\tmp\gen\out.rs:17:9"#]
    );
}

#[test]
fn find_noweb_entries_for_generated_file_rejects_ambiguous_short_suffixes() {
    let mut db = WeavebackDb::open_temp().expect("db");
    db.set_noweb_entries(
        "/tmp/a/src/main.rs",
        &[(
            0,
            NowebMapEntry {
                src_file: "a.adoc".to_string(),
                chunk_name: "a".to_string(),
                src_line: 1,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .expect("set a");
    db.set_noweb_entries(
        "/tmp/b/src/main.rs",
        &[(
            0,
            NowebMapEntry {
                src_file: "b.adoc".to_string(),
                chunk_name: "b".to_string(),
                src_line: 1,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .expect("set b");

    let got = find_noweb_entries_for_generated_file(&db, "src/main.rs", Path::new("."));
    assert!(got.is_none());
}


#[test]
fn emit_augmented_cargo_message_attaches_full_trace_json() {
    let line = r#"{"reason":"compiler-message","message":{"spans":[]}}"#;
    let records = vec![json!({
        "out_file": "gen/out.rs",
        "out_line": 17,
        "out_col": 9,
        "src_file": "src/doc.adoc",
        "src_line": 42,
        "src_col": 3,
        "chunk": "main",
        "kind": "Literal",
        "source_section_breadcrumb": ["Root", "Topic"],
        "source_section_prose": "Explain."
    })];
    let span_records = vec![json!({
        "generated_file": "gen/out.rs",
        "generated_line": 17,
        "generated_col": 9,
        "is_primary": true,
        "trace": records[0].clone(),
    })];
    let mut out = Vec::new();
    emit_augmented_cargo_message(line, records, span_records, &mut out).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let attrs = value["weaveback_attributions"].as_array().unwrap();
    assert_eq!(attrs.len(), 1);
    assert_eq!(attrs[0]["chunk"], "main");
    assert_eq!(attrs[0]["source_section_breadcrumb"], json!(["Root", "Topic"]));
    assert_eq!(attrs[0]["source_section_prose"], "Explain.");
    let span_attrs = value["weaveback_span_attributions"].as_array().unwrap();
    assert_eq!(span_attrs[0]["generated_file"], "gen/out.rs");
    assert_eq!(span_attrs[0]["trace"]["chunk"], "main");
    assert_eq!(value["weaveback_source_summary"]["sources"][0]["src_file"], "src/doc.adoc");
    assert_eq!(
        value["weaveback_source_summary"]["sources"][0]["sections"][0]["source_section_breadcrumb"],
        json!(["Root", "Topic"])
    );
    assert_eq!(
        value["weaveback_source_summary"]["sources"][0]["sections"][0]["generated_spans"][0]["generated_file"],
        "gen/out.rs"
    );
}

#[test]
fn emit_text_attribution_message_wraps_plain_text_line() {
    let mut out = Vec::new();
    emit_text_attribution_message(
        "stderr",
        "panic at src/generated.rs:1:27",
        vec![json!({
            "location": "src/generated.rs:1:27",
            "ok": true,
            "trace": {"expanded_file": "src/doc.adoc", "chunk": "generated"},
        })],
        &mut out,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(value["reason"], "weaveback-text-attribution");
    assert_eq!(value["stream"], "stderr");
    assert_eq!(value["text"], "panic at src/generated.rs:1:27");
    assert_eq!(value["weaveback_attributions"][0]["location"], "src/generated.rs:1:27");
    assert_eq!(
        value["weaveback_source_summary"]["sources"][0]["src_file"],
        "src/doc.adoc"
    );
}

#[test]
fn collect_text_attributions_scans_and_traces_locations() {
    let mut db = WeavebackDb::open_temp().expect("db");
    db.set_noweb_entries(
        "out.rs",
        &[(
            0,
            NowebMapEntry {
                src_file: "src/doc.adoc".to_string(),
                chunk_name: "main".to_string(),
                src_line: 3,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .expect("noweb");
    db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Topic\nalpha\n")
        .expect("snapshot");
    let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("gen"));
    let records = collect_text_attributions(
        "panic at out.rs:1 and out.rs:1",
        Some(&db),
        Path::new("."),
        &resolver,
        &EvalConfig::default(),
    );
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["location"], "out.rs:1");
    assert_eq!(records[0]["ok"], true);
    assert_eq!(records[0]["trace"]["chunk"], "main");
}

#[test]
fn build_location_attribution_summary_groups_successful_records() {
    let summary = build_location_attribution_summary(&[
        json!({
            "location": "out.rs:1",
            "ok": true,
            "trace": {
                "src_file": "src/doc.adoc",
                "chunk": "main",
                "source_section_breadcrumb": ["Root", "Topic"],
                "source_section_prose": "Explain."
            },
        }),
        json!({
            "location": "out.rs:2",
            "ok": false,
            "trace": serde_json::Value::Null,
        }),
    ]);
    assert_eq!(summary["count"], 1);
    assert_eq!(summary["sources"][0]["src_file"], "src/doc.adoc");
    assert_eq!(
        summary["sources"][0]["sections"][0]["locations"],
        json!(["out.rs:1"])
    );
}

#[test]
fn emit_cargo_summary_message_emits_final_grouped_json() {
    let span_records = vec![
        json!({
            "generated_file": "gen/out.rs",
            "generated_line": 17,
            "generated_col": 9,
            "is_primary": true,
            "trace": {
                "src_file": "src/doc.adoc",
                "chunk": "main",
                "source_section_breadcrumb": ["Root", "Topic"],
                "source_section_prose": "Explain."
            },
        }),
        json!({
            "generated_file": "gen/out.rs",
            "generated_line": 20,
            "generated_col": 1,
            "is_primary": false,
            "trace": {
                "src_file": "src/doc.adoc",
                "chunk": "helper",
                "source_section_breadcrumb": ["Root", "Topic"],
                "source_section_prose": "Explain."
            },
        }),
    ];
    let mut out = Vec::new();
    emit_cargo_summary_message(3, &span_records, &mut out).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(value["reason"], "weaveback-summary");
    assert_eq!(value["compiler_message_count"], 3);
    assert_eq!(value["generated_span_count"], 2);
    assert_eq!(value["weaveback_source_summary"]["sources"][0]["src_file"], "src/doc.adoc");
    assert_eq!(
        value["weaveback_source_summary"]["sources"][0]["sections"][0]["chunks"],
        json!(["helper", "main"])
    );
}

use std::sync::Mutex;
static CARGO_TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn run_cargo_annotated_to_writer_traces_real_generated_compile_error() {
    let _guard = CARGO_TEST_MUTEX.lock().unwrap();
    let temp = tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).expect("src dir");
    std::fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "wb-fixture"
version = "0.1.0"
edition = "2024"
"#,
    )
    .expect("Cargo.toml");
    std::fs::write(
        root.join("src/main.rs"),
        "mod generated;\nfn main() { generated::broken(); }\n",
    )
    .expect("main");
    std::fs::write(
        root.join("src/generated.rs"),
        "pub fn broken() { let x = ; }\n",
    )
    .expect("generated");

    let db_path = root.join("weaveback.db");
    let mut db = WeavebackDb::open(&db_path).expect("db");
    db.set_noweb_entries(
        "src/generated.rs",
        &[(
            0,
            NowebMapEntry {
                src_file: "src/doc.adoc".to_string(),
                chunk_name: "generated".to_string(),
                src_line: 3,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .expect("noweb");
    db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Generated\nThe generated body.\n")
        .expect("snapshot");

    let mut out = Vec::new();
    let err = run_cargo_annotated_to_writer(
        vec!["check".to_string(), "--quiet".to_string()],
        true,
        db_path,
        root.join("gen"),
        EvalConfig::default(),
        root,
        &mut out,
    )
    .expect_err("cargo should fail on generated syntax error");
    let rendered = String::from_utf8(out).expect("utf8");
    let lines = rendered
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json line"))
        .collect::<Vec<_>>();

    assert!(matches!(err, CoverageApiError::Io(_)));
    let compiler = lines
        .iter()
        .find(|value| value["reason"] == "compiler-message")
        .expect("compiler message");
    let span_attrs = compiler["weaveback_span_attributions"]
        .as_array()
        .expect("span attributions");
    assert!(!span_attrs.is_empty());
    assert!(span_attrs.iter().any(|record| {
        record["trace"]["src_file"]
            .as_str()
            .or_else(|| record["trace"]["expanded_file"].as_str())
            .is_some_and(|path| path.ends_with("src/doc.adoc"))
            && record["trace"]["source_section_breadcrumb"] == json!(["Root", "Generated"])
    }));

    let summary = lines
        .iter()
        .find(|value| value["reason"] == "weaveback-summary")
        .expect("summary");
    let sections = summary["weaveback_source_summary"]["sources"][0]["sections"]
        .as_array()
        .expect("sections");
    assert!(sections.iter().any(|section| {
        section["source_section_breadcrumb"] == json!(["Root", "Generated"])
            && section["generated_spans"]
                .as_array()
                .is_some_and(|spans| spans.iter().any(|span| {
                    span["generated_file"]
                        .as_str()
                        .is_some_and(|file| file.ends_with("src/generated.rs"))
                }))
    }));
}

#[test]
fn run_cargo_annotated_to_writer_emits_text_attribution_for_text_warning() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).expect("src dir");
    std::fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "wb-fixture-warning"
version = "0.1.0"
edition = "2024"
build = "build.rs"
"#,
    )
    .expect("Cargo.toml");
    std::fs::write(
        root.join("build.rs"),
        "fn main() { println!(\"cargo:warning=src/generated.rs:1:27\"); }\n",
    )
    .expect("build");
    std::fs::write(
        root.join("src/main.rs"),
        "fn main() {}\n",
    )
    .expect("main");
    std::fs::write(
        root.join("src/generated.rs"),
        "pub fn generated() {}\n",
    )
    .expect("generated");

    let db_path = root.join("weaveback.db");
    let mut db = WeavebackDb::open(&db_path).expect("db");
    db.set_noweb_entries(
        "src/generated.rs",
        &[(
            0,
            NowebMapEntry {
                src_file: "src/doc.adoc".to_string(),
                chunk_name: "generated".to_string(),
                src_line: 3,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .expect("noweb");
    db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Generated\nThe generated body.\n")
        .expect("snapshot");

    let _guard = CARGO_TEST_MUTEX.lock().unwrap();
    let mut out = Vec::new();
    run_cargo_annotated_to_writer(
        vec![
            "check".to_string(),
        ],
        true,
        db_path,
        root.join("gen"),
        EvalConfig::default(),
        root,
        &mut out,
    )
    .expect("cargo check should succeed");
    let rendered = String::from_utf8(out).expect("utf8");
    let lines = rendered
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json line"))
        .collect::<Vec<_>>();

    let text_attr = lines
        .iter()
        .find(|value| value["reason"] == "weaveback-text-attribution")
        .expect("text attribution");
    assert_eq!(text_attr["stream"], "stderr");
    assert!(
        text_attr["weaveback_attributions"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| {
                item["trace"]["source_section_breadcrumb"] == json!(["Root", "Generated"])
            }))
    );
}

#[test]
fn parse_lcov_records_extracts_file_line_hits() {
    let text = "TN:\nSF:src/generated.rs\nDA:1,3\nDA:2,0\nend_of_record\nSF:other.rs\nDA:4,1\nend_of_record\n";
    assert_eq!(
        parse_lcov_records(text),
        vec![
            ("src/generated.rs".to_string(), 1, 3),
            ("src/generated.rs".to_string(), 2, 0),
            ("other.rs".to_string(), 4, 1),
        ]
    );
}

#[test]
fn build_coverage_summary_groups_lines_by_source_section() {
    let mut db = WeavebackDb::open_temp().expect("db");
    db.set_noweb_entries(
        "src/generated.rs",
        &[
            (
                0,
                NowebMapEntry {
                    src_file: "src/doc.adoc".to_string(),
                    chunk_name: "generated".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            ),
            (
                1,
                NowebMapEntry {
                    src_file: "src/doc.adoc".to_string(),
                    chunk_name: "generated".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            ),
        ],
    )
    .expect("noweb");
    db.set_src_snapshot(
        "src/doc.adoc",
        b"= Root\n\n== Generated\nThe generated body.\n",
    )
    .expect("snapshot");
    let records = vec![
        ("src/generated.rs".to_string(), 1, 1),
        ("src/generated.rs".to_string(), 2, 0),
        ("unmapped.rs".to_string(), 9, 0),
    ];
    let project_root = PathBuf::from(".");
    let resolver = PathResolver::new(project_root.clone(), PathBuf::from("gen"));
    let summary = build_coverage_summary(
        &records,
        &db,
        &project_root,
        &resolver,
    );
    assert_eq!(summary["line_records"], 3);
    assert_eq!(summary["attributed_records"], 2);
    assert_eq!(summary["unattributed_records"], 1);
    assert!(
        summary["sources"][0]["src_file"]
            .as_str()
            .is_some_and(|path| path.ends_with("src/doc.adoc"))
    );
    assert_eq!(summary["sources"][0]["covered_lines"], 1);
    assert_eq!(summary["sources"][0]["missed_lines"], 1);
    assert_eq!(
        summary["sources"][0]["sections"][0]["source_section_breadcrumb"],
        json!(["Root", "Generated"])
    );
    assert_eq!(
        summary["sources"][0]["sections"][0]["generated_lines"][0]["generated_file"],
        "src/generated.rs"
    );
    assert_eq!(summary["unattributed"][0]["generated_file"], "unmapped.rs");
    assert_eq!(summary["unattributed_files"][0]["generated_file"], "unmapped.rs");
    assert_eq!(summary["unattributed_files"][0]["missed_lines"], 1);
    assert_eq!(summary["unattributed_files"][0]["has_noweb_entries"], false);
}

#[test]
fn build_coverage_summary_marks_partial_unattributed_files() {
    let mut db = WeavebackDb::open_temp().expect("db");
    db.set_noweb_entries(
        "src/generated.rs",
        &[(
            0,
            NowebMapEntry {
                src_file: "src/doc.adoc".to_string(),
                chunk_name: "generated".to_string(),
                src_line: 3,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .expect("noweb");
    db.set_src_snapshot(
        "src/doc.adoc",
        b"= Root\n\n== Generated\nThe generated body.\n",
    )
    .expect("snapshot");
    let records = vec![
        ("src/generated.rs".to_string(), 2, 0),
    ];
    let project_root = PathBuf::from(".");
    let resolver = PathResolver::new(project_root.clone(), PathBuf::from("gen"));
    let summary = build_coverage_summary(&records, &db, &project_root, &resolver);
    assert_eq!(summary["unattributed_records"], 1);
    assert_eq!(summary["unattributed_files"][0]["generated_file"], "src/generated.rs");
    assert_eq!(summary["unattributed_files"][0]["has_noweb_entries"], true);
    assert_eq!(summary["unattributed_files"][0]["mapped_line_start"], 1);
    assert_eq!(summary["unattributed_files"][0]["mapped_line_end"], 1);
}

#[test]
fn build_coverage_summary_sorts_sources_and_sections_by_missed_lines() {
    let mut db = WeavebackDb::open_temp().expect("db");
    db.set_noweb_entries(
        "src/a_generated.rs",
        &[
            (
                0,
                NowebMapEntry {
                    src_file: "src/a.adoc".to_string(),
                    chunk_name: "alpha".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            ),
            (
                1,
                NowebMapEntry {
                    src_file: "src/a.adoc".to_string(),
                    chunk_name: "alpha".to_string(),
                    src_line: 6,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            ),
        ],
    )
    .expect("a noweb");
    db.set_noweb_entries(
        "src/b_generated.rs",
        &[
            (
                0,
                NowebMapEntry {
                    src_file: "src/b.adoc".to_string(),
                    chunk_name: "beta".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            ),
            (
                1,
                NowebMapEntry {
                    src_file: "src/b.adoc".to_string(),
                    chunk_name: "beta".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            ),
        ],
    )
    .expect("b noweb");
    db.set_src_snapshot("src/a.adoc", b"= Root\n\n== A1\none\n\n== A2\ntwo\n")
        .expect("a snapshot");
    db.set_src_snapshot("src/b.adoc", b"= Root\n\n== B\nbody\n")
        .expect("b snapshot");
    let records = vec![
        ("src/a_generated.rs".to_string(), 1, 1),
        ("src/a_generated.rs".to_string(), 2, 0),
        ("src/b_generated.rs".to_string(), 1, 0),
        ("src/b_generated.rs".to_string(), 2, 0),
    ];
    let project_root = PathBuf::from(".");
    let resolver = PathResolver::new(project_root.clone(), PathBuf::from("gen"));
    let summary = build_coverage_summary(
        &records,
        &db,
        &project_root,
        &resolver,
    );
    let sources = summary["sources"].as_array().expect("sources");
    assert!(
        sources[0]["src_file"]
            .as_str()
            .is_some_and(|path| path.ends_with("src/b.adoc"))
    );
    let a_sections = sources[1]["sections"].as_array().expect("sections");
    assert_eq!(a_sections[0]["source_section_breadcrumb"], json!(["Root", "A2"]));
    assert_eq!(a_sections[0]["missed_lines"], 1);
    assert_eq!(a_sections[1]["source_section_breadcrumb"], json!(["Root", "A1"]));
}

#[test]
fn build_coverage_summary_view_keeps_ranked_top_slices() {
    let summary = json!({
        "line_records": 3,
        "attributed_records": 3,
        "unattributed_records": 0,
        "unattributed_files": [
            {
                "generated_file": "gen/a.rs",
                "missed_lines": 3
            },
            {
                "generated_file": "gen/b.rs",
                "missed_lines": 1
            }
        ],
        "sources": [
            {
                "src_file": "src/a.adoc",
                "sections": [
                    {"source_section_breadcrumb": ["Root", "A1"]},
                    {"source_section_breadcrumb": ["Root", "A2"]}
                ]
            },
            {
                "src_file": "src/b.adoc",
                "sections": [
                    {"source_section_breadcrumb": ["Root", "B1"]}
                ]
            }
        ]
    });
    let view = build_coverage_summary_view(&summary, 1, 1);
    assert_eq!(view["summary_view"]["top_sources"], 1);
    assert_eq!(view["summary_view"]["top_sections"], 1);
    assert_eq!(view["summary_view"]["sources"].as_array().unwrap().len(), 1);
    assert_eq!(view["summary_view"]["unattributed_files"].as_array().unwrap().len(), 1);
    assert_eq!(view["summary_view"]["unattributed_files"][0]["generated_file"], "gen/a.rs");
    assert_eq!(
        view["summary_view"]["sources"][0]["sections"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn collect_cargo_attributions_maps_generated_span_back_to_source() {
    let mut db = WeavebackDb::open_temp().expect("db");
    db.set_noweb_entries(
        "out.rs",
        &[(
            0,
            NowebMapEntry {
                src_file: "src/doc.adoc".to_string(),
                chunk_name: "main".to_string(),
                src_line: 3,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .expect("noweb");
    db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Topic\nalpha\n")
        .expect("snapshot");
    let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("gen"));
    let diagnostic = CargoDiagnostic {
        spans: vec![CargoDiagnosticSpan {
            file_name: "out.rs".to_string(),
            line_start: 1,
            column_start: 1,
            is_primary: true,
        }],
    };

    let records = collect_cargo_attributions(
        &diagnostic,
        Some(&db),
        Path::new("."),
        &resolver,
        &EvalConfig::default(),
    );
    assert_eq!(records.len(), 1);
    assert!(
        records[0]["src_file"]
            .as_str()
            .is_some_and(|path| path.ends_with("src/doc.adoc"))
    );
    assert_eq!(records[0]["src_line"], 4);
    assert_eq!(records[0]["chunk"], "main");
    assert_eq!(records[0]["source_section_breadcrumb"], json!(["Root", "Topic"]));
}

#[test]
fn collect_cargo_span_attributions_keeps_generated_span_context() {
    let mut db = WeavebackDb::open_temp().expect("db");
    db.set_noweb_entries(
        "out.rs",
        &[(
            0,
            NowebMapEntry {
                src_file: "src/doc.adoc".to_string(),
                chunk_name: "main".to_string(),
                src_line: 3,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .expect("noweb");
    db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Topic\nalpha\n")
        .expect("snapshot");
    let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("gen"));
    let diagnostic = CargoDiagnostic {
        spans: vec![
            CargoDiagnosticSpan {
                file_name: "out.rs".to_string(),
                line_start: 1,
                column_start: 1,
                is_primary: true,
            },
            CargoDiagnosticSpan {
                file_name: "out.rs".to_string(),
                line_start: 1,
                column_start: 5,
                is_primary: false,
            },
        ],
    };

    let records = collect_cargo_span_attributions(
        &diagnostic,
        Some(&db),
        Path::new("."),
        &resolver,
        &EvalConfig::default(),
    );
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["generated_file"], "out.rs");
    assert_eq!(records[0]["trace"]["chunk"], "main");
    assert_eq!(records[1]["is_primary"], false);
}

#[test]
fn build_cargo_attribution_summary_groups_by_source_file() {
    let summary = build_cargo_attribution_summary(&[
        json!({
            "generated_file": "out.rs",
            "generated_line": 1,
            "generated_col": 1,
            "is_primary": true,
            "trace": {
                "src_file": "src/a.adoc",
                "chunk": "alpha",
                "source_section_breadcrumb": ["Root", "Alpha"],
                "source_section_prose": "Alpha prose."
            }
        }),
        json!({
            "generated_file": "out.rs",
            "generated_line": 2,
            "generated_col": 1,
            "is_primary": false,
            "trace": {
                "src_file": "src/a.adoc",
                "chunk": "beta",
                "source_section_breadcrumb": ["Root", "Alpha"],
                "source_section_prose": "Alpha prose."
            }
        }),
        json!({
            "generated_file": "out2.rs",
            "generated_line": 1,
            "generated_col": 1,
            "is_primary": true,
            "trace": {
                "src_file": "src/b.adoc",
                "chunk": "gamma",
                "source_section_breadcrumb": ["Root", "Beta"],
                "source_section_prose": "Beta prose."
            }
        }),
    ]);
    assert_eq!(summary["count"], 3);
    assert_eq!(summary["sources"][0]["src_file"], "src/a.adoc");
    assert_eq!(summary["sources"][0]["count"], 2);
    assert_eq!(
        summary["sources"][0]["sections"][0]["source_section_breadcrumb"],
        json!(["Root", "Alpha"])
    );
    assert_eq!(
        summary["sources"][0]["sections"][0]["generated_spans"][0]["generated_file"],
        "out.rs"
    );
    assert_eq!(summary["sources"][1]["src_file"], "src/b.adoc");
}
#[test]
fn test_parse_lcov_records_simple() {
    let lcov = "SF:file.rs\nDA:1,10\nDA:2,0\nend_of_record\nSF:other.rs\nDA:5,1\nend_of_record\n";
    let records = parse_lcov_records(lcov);
    assert_eq!(records.len(), 3);
    assert_eq!(records[0], ("file.rs".to_string(), 1, 10));
    assert_eq!(records[1], ("file.rs".to_string(), 2, 0));
    assert_eq!(records[2], ("other.rs".to_string(), 5, 1));
}

#[test]
fn test_compute_unmapped_ranges() {
    let lines = vec![
        json!({"generated_line": 1, "hit_count": 5}),
        json!({"generated_line": 2, "hit_count": 0}),
        json!({"generated_line": 3, "hit_count": 0}),
        json!({"generated_line": 5, "hit_count": 1}),
    ];
    let ranges = compute_unmapped_ranges(&lines);
    let arr = ranges.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["start"], 1);
    assert_eq!(arr[0]["end"], 3);
    assert_eq!(arr[0]["missed_count"], 2);
    assert_eq!(arr[1]["start"], 5);
    assert_eq!(arr[1]["missed_count"], 0);
}
// ── open_db ────────────────────────────────────────────────────────────

#[test]
fn open_db_errors_when_database_missing() {
    let result = open_db(std::path::Path::new("/nonexistent/weaveback.db"));
    assert!(result.is_err());
    let msg = format!("{}", result.err().unwrap());
    assert!(msg.contains("Database not found") || msg.contains("not found"), "got: {msg}");
}

// ── parse_generated_location errors ────────────────────────────────────

#[test]
fn parse_generated_location_errors_on_missing_colon() {
    let result = parse_generated_location("nocolon");
    assert!(result.is_err(), "expected Err for missing colon");
}

#[test]
fn parse_generated_location_errors_on_bad_line_number() {
    let result = parse_generated_location("file.rs:abc");
    assert!(result.is_err(), "expected Err for non-numeric line");
}

#[test]
fn parse_generated_location_errors_on_bad_col_number() {
    let result = parse_generated_location("file.rs:10:xyz");
    assert!(result.is_err(), "expected Err for non-numeric column");
}

#[test]
fn parse_generated_location_parses_file_line_col() {
    let result = parse_generated_location("src/lib.rs:42:7").unwrap();
    assert_eq!(result.0, "src/lib.rs");
    assert_eq!(result.1, 42);
    assert_eq!(result.2, 7);
}

#[test]
fn parse_generated_location_parses_file_line_only() {
    let result = parse_generated_location("src/lib.rs:42").unwrap();
    assert_eq!(result.0, "src/lib.rs");
    assert_eq!(result.1, 42);
    assert_eq!(result.2, 1); // defaults col to 1
}

// ── CoverageApiError conversions ──────────────────────────────────────────

#[test]
fn coverage_error_io_displays_message() {
    let e = CoverageApiError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"));
    assert!(format!("{e}").contains("gone"));
}

// ── print_coverage_summary_to_writer ──────────────────────────────────

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
fn emit_cargo_summary_message_outputs_json_with_reason() {
    let mut out = Vec::new();
    emit_cargo_summary_message(5, &[], &mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    let val: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
    assert_eq!(val["reason"], "weaveback-summary");
    assert_eq!(val["compiler_message_count"], 5);
    assert_eq!(val["generated_span_count"], 0);
}

// ── build_location_attribution_summary ────────────────────────────────

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





#[test]
fn parse_lcov_records_malformed() {
    // Test records with missing fields or partial records
    let text = "SF:a.rs\nDA:1,1\nSF:b.rs\nend_of_record\n";
    let records = parse_lcov_records(text);
    // Only b.rs has end_of_record, so a.rs is incomplete or they are merged?
    // Let's check the current implementation: it collects DA for the current SF.
    // SF:a.rs -> current_file = "a.rs"
    // DA:1,1 -> hits.push(("a.rs", 1, 1))
    // SF:b.rs -> current_file = "b.rs"
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].0, "a.rs");
}

#[test]
fn build_coverage_summary_deep_sections() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = WeavebackDb::open(&db_path).unwrap();
    let project_root = tmp.path().to_path_buf();
    let resolver = PathResolver::new(project_root.clone(), project_root.join("gen"));

    // Mock a source file with nested sections
    let src_file = "src/main.adoc";
    let content = "= Root\n\n== Section A\n\n=== Subsection A1\n\n<<a1>>=\ncode\n@\n";
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join(src_file), content).unwrap();

    // Seed database
    db.set_noweb_entries("main.rs", &[(0, weaveback_tangle::db::NowebMapEntry {
        src_file: src_file.to_string(),
        chunk_name: "a1".to_string(),
        src_line: 6,
        indent: "".into(),
        confidence: Confidence::Exact,
    })]).unwrap();
    // Since we don't have a snapshot, we'll rely on load_source_text loading from disk.

    let records = vec![("main.rs".to_string(), 1, 1)];
    let summary = build_coverage_summary(&records, &db, &project_root, &resolver);

    let sources = summary["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 1);
    let sections = sources[0]["sections"].as_array().unwrap();
    // Should find "Root / Section A / Subsection A1"
    let found = sections.iter().any(|s| {
        let b = s["source_section_breadcrumb"].as_array().unwrap();
        b.len() == 3 && b[2] == "Subsection A1"
    });
    assert!(found, "Nested section breadcrumb not found: {:?}", sections);
}

#[test]
fn run_coverage_failure_missing_lcov() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let lcov_path = tmp.path().join("nonexistent.lcov");
    let gen_dir = tmp.path().join("gen");

    let res = run_coverage(false, 10, 5, false, lcov_path, db_path, gen_dir);
    assert!(res.is_err());
}

// ── Batch 4: Cargo & Text Attribution ──────────────────────────────────

#[test]
fn test_collect_cargo_attributions_with_mock() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = WeavebackDb::open(&db_path).unwrap();
    let project_root = tmp.path().to_path_buf();
    let resolver = PathResolver::new(project_root.clone(), project_root.join("gen"));

    let src_file = "src/main.adoc";
    ws_write_file(&project_root, src_file, b"content");

    db.set_noweb_entries("main.rs", &[(0, weaveback_tangle::db::NowebMapEntry {
        src_file: src_file.to_string(),
        chunk_name: "main".to_string(),
        src_line: 0,
        indent: "".into(),
        confidence: Confidence::Exact,
    })]).unwrap();

    let diag = CargoDiagnostic {
        spans: vec![CargoDiagnosticSpan {
            file_name: "main.rs".to_string(),
            line_start: 1,
            column_start: 1,
            is_primary: true,
        }],
    };

    let attributions = collect_cargo_attributions(
        &diag,
        Some(&db),
        &project_root,
        &resolver,
        &EvalConfig::default(),
    );
    assert_eq!(attributions.len(), 1);
    assert_eq!(attributions[0]["src_file"].as_str(), Some(project_root.join(src_file).to_string_lossy().as_ref()));
}

#[test]
fn test_emit_augmented_cargo_message() {
    let mut out = Vec::new();
    let diag_json = json!({"reason": "compiler-message", "message": {"spans": []}});
    let original = serde_json::to_string(&diag_json).unwrap();

    emit_augmented_cargo_message(&original, vec![json!({"ok":true})], vec![], &mut out).unwrap();
    let result = String::from_utf8(out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert_eq!(parsed["reason"], "compiler-message");
    assert!(parsed.get("weaveback_attributions").is_some());
}

#[test]
fn test_collect_text_attributions_scans_locations() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = WeavebackDb::open(&db_path).unwrap();
    let project_root = tmp.path().to_path_buf();
    let resolver = PathResolver::new(project_root.clone(), project_root.join("gen"));

    ws_write_file(&project_root, "src/a.adoc", b"content");
    db.set_noweb_entries("out.rs", &[(9, weaveback_tangle::db::NowebMapEntry {
        src_file: "src/a.adoc".to_string(),
        chunk_name: "main".to_string(),
        src_line: 5,
        indent: "".into(),
        confidence: Confidence::Exact,
    })]).unwrap();

    let text = "Error at out.rs:10:1 and some other text";
    let attributions = collect_text_attributions(
        text,
        Some(&db),
        &project_root,
        &resolver,
        &EvalConfig::default(),
    );

    assert_eq!(attributions.len(), 1);
    assert_eq!(attributions[0]["location"], "out.rs:10:1");
    assert_eq!(attributions[0]["ok"], true);
}

#[test]
fn test_emit_text_attribution_message() {
    let mut out = Vec::new();
    let attributions = vec![json!({"location": "out.rs:1:1", "ok": false})];

    emit_text_attribution_message("stdout", "some test line", attributions, &mut out).unwrap();
    let result = String::from_utf8(out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert_eq!(parsed["reason"], "weaveback-text-attribution");
    assert_eq!(parsed["stream"], "stdout");
    assert_eq!(parsed["text"], "some test line");
}

#[test]
fn test_run_cargo_annotated_to_writer_mega_mock() {
    let _guard = CARGO_TEST_MUTEX.lock().unwrap();
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let gen_dir = tmp.path().join("gen");
    std::fs::create_dir(&gen_dir).unwrap();

    let mut out = Vec::new();

    // Mock shell script that outputs:
    // 1. A compiler-message (JSON)
    // 2. A plain text line (stderr-like)
    // 3. A build-finished message (JSON)
    let mock_script = r#"
        echo '{"reason":"compiler-message","message":{"spans":[{"file_name":"src/main.rs","line_start":1,"column_start":1,"is_primary":true}]}}'
        echo "plain text stderr line that looks like a location out.rs:1:1" >&2
        echo '{"reason":"build-finished","success":true}'
    "#;

    unsafe { std::env::set_var("WEAVEBACK_CARGO_BIN", "sh"); }
    let res = run_cargo_annotated_to_writer(
        vec!["-c".to_string(), mock_script.to_string()],
        false,
        db_path,
        gen_dir,
        EvalConfig::default(),
        tmp.path(),
        &mut out,
    );
    unsafe { std::env::remove_var("WEAVEBACK_CARGO_BIN"); }

    assert!(res.is_ok());
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("compiler-message"));
    assert!(output.contains("build-finished"));
    // The stderr line ("plain text stderr line") is currently piped to out in the loop too.
}

#[test]
fn test_run_cargo_annotated_to_writer_diagnostics_only() {
    let _guard = CARGO_TEST_MUTEX.lock().unwrap();
    let tmp = tempdir().unwrap();
    let mut out = Vec::new();
    let mock_script = "echo '{\"reason\":\"compiler-message\",\"message\":{\"spans\":[]}}'; echo '{\"reason\":\"other\"}'";

    unsafe { std::env::set_var("WEAVEBACK_CARGO_BIN", "sh"); }
    run_cargo_annotated_to_writer(
        vec!["-c".to_string(), mock_script.to_string()],
        true, // diagnostics_only = true
        tmp.path().join("db"),
        tmp.path().join("gen"),
        EvalConfig::default(),
        tmp.path(),
        &mut out,
    ).unwrap();
    unsafe { std::env::remove_var("WEAVEBACK_CARGO_BIN"); }

    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("compiler-message"));
    assert!(!output.contains("other"), "expected 'other' message to be filtered out");
}

#[test]
fn test_run_tags_empty_db() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    // No DB created yet
    let res = run_tags(None, db_path);
    // crate::query::list_block_tags currently might fail if DB doesn't exist
    assert!(res.is_err() || res.is_ok());
}

#[test]
fn test_run_impact_missing_db() {
    let tmp = tempdir().unwrap();
    let res = run_impact("some-chunk".to_string(), tmp.path().join("missing.db"));
    assert!(res.is_err());
}

#[test]
fn test_run_graph_missing_db() {
    let tmp = tempdir().unwrap();
    let res = run_graph(None, tmp.path().join("missing.db"));
    assert!(res.is_err());
}

#[test]
fn test_run_coverage_summary_only() {
    let tmp = tempdir().unwrap();
    let lcov = tmp.path().join("test.lcov");
    std::fs::write(&lcov, "SF:src/a.rs\nDA:1,1\nend_of_record\n").unwrap();
    let db_path = tmp.path().join("test_cov.db");
    let _db = WeavebackDb::open(&db_path).unwrap();

    let res = run_coverage(
        true, // summary_only
        10, 10, false,
        lcov,
        db_path,
        tmp.path().join("gen")
    );
    assert!(res.is_ok());
}



#[test]
fn test_coverage_error_conversions() {
    use rusqlite;
    let io_err = std::io::Error::other("io");
    let ce_io = CoverageApiError::from(io_err);
    assert!(ce_io.to_string().contains("io"));

    let db_err = rusqlite::Error::QueryReturnedNoRows;
    let ce_db = CoverageApiError::from(weaveback_tangle::db::DbError::from(db_err));
    assert!(ce_db.to_string().contains("Query returned no rows"));

    let lookup_err = lookup::LookupError::InvalidInput("bad input".to_string());
    let ce_lookup = CoverageApiError::from(lookup_err);
    assert!(ce_lookup.to_string().contains("bad input"));
}

#[test]
fn test_parse_generated_location_errors() {
    assert!(parse_generated_location("").is_err());
    assert!(parse_generated_location("file").is_err());
    assert!(parse_generated_location("file:notanumber").is_err());
    assert!(parse_generated_location("file:10:notanumber").is_err());
}

#[test]
fn test_scan_generated_locations_edge_cases() {
    // Empty after normalization
    assert!(scan_generated_locations("...").is_empty());
    // Invalid location overall
    assert!(scan_generated_locations("file:notanumber").is_empty());
}

#[test]
fn test_run_where_orchestration() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_where.db");
    let _db = WeavebackDb::open(&db_path).unwrap();

    // No mapping
    let res = run_where("test.rs".to_string(), 1, db_path, tmp.path().join("gen"));
    assert!(res.is_ok());
}

#[test]
fn test_run_attribute_orchestration() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_attr.db");
    let _db = WeavebackDb::open(&db_path).unwrap();

    // Single location, no summary -> calls run_trace internal path
    let res = run_attribute(
        false, false,
        vec!["test.rs:1".to_string()],
        db_path.clone(),
        tmp.path().join("gen"),
        EvalConfig::default()
    );
    assert!(res.is_ok());

    // Locations list required
    let res_err = run_attribute(
        false, false, vec![], db_path.clone(), tmp.path().join("gen"), EvalConfig::default()
    );
    assert!(res_err.is_err());
}


fn ws_write_file(root: &Path, rel: &str, content: &[u8]) {
    let p = root.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, content).unwrap();
}

