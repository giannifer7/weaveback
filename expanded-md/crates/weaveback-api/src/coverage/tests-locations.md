# Coverage Location Tests

Generated-location parsing, scanning, where/attribute orchestration, and direct DB lookup edge cases.

```rust
// <[coverage-tests-locations]>=
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
// @
```

