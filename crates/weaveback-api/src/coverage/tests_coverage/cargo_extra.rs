// weaveback-api/src/coverage/tests_coverage/cargo_extra.rs
// I'd Really Rather You Didn't edit this generated file.

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

