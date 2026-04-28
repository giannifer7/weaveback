// weaveback-api/src/coverage/tests_coverage/lcov_summary.rs
// I'd Really Rather You Didn't edit this generated file.

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

