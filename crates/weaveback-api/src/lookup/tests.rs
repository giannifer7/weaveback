// weaveback-api/src/lookup/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use std::path::PathBuf;
use weaveback_tangle::db::{Confidence, NowebMapEntry};

fn resolver() -> PathResolver {
    PathResolver::new(PathBuf::from("."), PathBuf::from("gen"))
}

#[test]
fn perform_where_validates_line_and_returns_none_when_unmapped() {
    let db = WeavebackDb::open_temp().expect("db");
    let resolver = resolver();

    let err = perform_where("out.rs", 0, &db, &resolver).expect_err("invalid line");
    assert!(matches!(err, LookupError::InvalidInput(_)));
    assert!(perform_where("out.rs", 1, &db, &resolver).expect("lookup").is_none());
}

#[test]
fn perform_where_returns_normalized_mapping() {
    let mut db = WeavebackDb::open_temp().expect("db");
    db.set_noweb_entries(
        "out.rs",
        &[(
            0,
            NowebMapEntry {
                src_file: "src/doc.adoc".to_string(),
                chunk_name: "main".to_string(),
                src_line: 4,
                indent: "    ".to_string(),
                confidence: Confidence::HashMatch,
            },
        )],
    )
    .expect("noweb");

    let value = perform_where("gen/out.rs", 1, &db, &resolver())
        .expect("where")
        .expect("mapped");
    assert_eq!(value["chunk"], "main");
    assert_eq!(value["expanded_file"], "src/doc.adoc");
    assert_eq!(value["expanded_line"], 5);
    assert_eq!(value["confidence"], "hash_match");
}

#[test]
fn append_def_locations_uses_snapshots_for_line_and_column() {
    let db = WeavebackDb::open_temp().expect("db");
    db.set_src_snapshot("src/doc.adoc", b"first\nlet answer = 42;\n")
        .expect("snapshot");
    db.record_var_def("answer", "src/doc.adoc", 10, 6)
        .expect("var def");
    db.record_macro_def("emit", "src/doc.adoc", 6, 3)
        .expect("macro def");

    let mut obj = serde_json::Map::new();
    append_def_locations(&mut obj, "set_locations", "answer", &db, true);
    append_def_locations(&mut obj, "def_locations", "emit", &db, false);

    let set_locations = obj["set_locations"].as_array().expect("set locations");
    assert_eq!(set_locations.len(), 1);
    assert_eq!(set_locations[0]["file"], "src/doc.adoc");
    assert_eq!(set_locations[0]["line"], 2);
    assert_eq!(set_locations[0]["col"], 5);

    let def_locations = obj["def_locations"].as_array().expect("def locations");
    assert_eq!(def_locations.len(), 1);
    assert_eq!(def_locations[0]["line"], 2);
}

#[test]
fn perform_trace_uses_snapshot_and_adds_literal_source_location() {
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
    db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Trace\nalpha\n")
        .expect("snapshot");

    let traced = perform_trace("out.rs", 1, 1, &db, &resolver(), EvalConfig::default())
        .expect("trace")
        .expect("value");

    assert_eq!(traced["chunk"], "main");
    assert_eq!(traced["expanded_file"], "src/doc.adoc");
    assert_eq!(traced["src_line"], 4);
    assert_eq!(traced["src_col"], 1);
    assert_eq!(traced["kind"], "Literal");
    assert_eq!(traced["source_section_breadcrumb"], json!(["Root", "Trace"]));
    assert_eq!(
        traced["source_section_range"],
        json!({ "start_line": 3, "end_line": 4 })
    );
    assert_eq!(traced["source_section_prose"], "== Trace\nalpha");
}

#[test]
fn perform_trace_coarse_adds_context_without_precise_span_fields() {
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
    db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Trace\nalpha\n")
        .expect("snapshot");

    let traced = perform_trace_coarse("out.rs", 1, &db, &resolver())
        .expect("trace")
        .expect("value");

    assert_eq!(traced["chunk"], "main");
    assert_eq!(traced["expanded_file"], "src/doc.adoc");
    assert_eq!(traced["source_section_breadcrumb"], json!(["Root", "Trace"]));
    assert!(traced.get("src_line").is_none());
    assert!(traced.get("src_col").is_none());
    assert!(traced.get("kind").is_none());
}

#[test]
fn append_source_context_skips_chunk_bodies_and_fences() {
    let src = [
        "= Root",
        "",
        "== Topic",
        "Intro.",
        "",
        "----",
        "code",
        "----",
        "",
        "// <<main>>=",
        "generated-ish",
        "// @",
        "",
        "Tail.",
    ]
    .join("\n");
    let mut obj = serde_json::Map::new();
    append_source_context(&mut obj, &src, 14);

    assert_eq!(obj["source_section_breadcrumb"], json!(["Root", "Topic"]));
    assert_eq!(obj["source_section_prose"], "== Topic\nIntro.\n\nTail.");
}

#[test]
fn perform_trace_validates_line_before_db_lookup() {
    let db = WeavebackDb::open_temp().expect("db");
    let err = perform_trace("out.rs", 0, 1, &db, &resolver(), EvalConfig::default())
        .expect_err("invalid line");
    assert!(matches!(err, LookupError::InvalidInput(_)));
}

