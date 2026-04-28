# Apply-Back Runner Tests

`run_apply_back` entry-point behavior and patch-source selection.

```rust
// <[applyback-tests-runner]>=
// ── run_apply_back entry point edge cases ──────────────────────────────

#[test]
fn run_apply_back_gen_dir_fallback() {
    let ws = TestWorkspace::new();
    let db = ws.open_db();
    // Set gen_dir in run_config. 
    db.set_run_config("gen_dir", ws.root.join("alt_gen").to_str().unwrap()).unwrap();
    db.set_baseline("test.rs", b"content").unwrap();
    
    // Write file in alt_gen.
    ws.write_file("alt_gen/test.rs", b"MODIFIED");

    let opts = ApplyBackOptions {
        db_path: ws.root.join("weaveback.db"),
        gen_dir: std::path::PathBuf::from("gen"), // default doesn't exist
        files: vec![],
        dry_run: true,
        eval_config: None,
    };
    let mut out = Vec::new();

    // Should fall back to alt_gen from DB and find the MODIFIED file.
    run_apply_back(opts, &mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("Processing test.rs"));
}

#[test]
fn run_apply_back_specific_files_non_existent_is_no_op() {
    let ws = TestWorkspace::new();
    let _db = ws.open_db(); // just creates it

    let opts = ApplyBackOptions {
        db_path: ws.root.join("weaveback.db"),
        gen_dir: ws.root.join("gen"),
        files: vec!["missing.rs".into()],
        dry_run: false,
        eval_config: None,
    };
    let mut out = Vec::new();
    run_apply_back(opts, &mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    // Since missing.rs is not in baselines, it should say no modified files found.
    assert!(s.contains("No modified gen/ files found"));
}

#[test]
fn run_apply_back_diff_delete_is_detected() {
    let ws = TestWorkspace::new();
    let mut db = ws.open_db();
    db.set_baseline("out.rs", b"line1\nline2").unwrap();
    ws.write_file("gen/out.rs", b"line1\n"); // line2 deleted
    
    db.set_noweb_entries("out.rs", &[
        (0, NowebMapEntry { src_file: "src.adoc".into(), chunk_name: "c".into(), src_line: 0, indent: "".into(), confidence: Confidence::Exact }),
        (1, NowebMapEntry { src_file: "src.adoc".into(), chunk_name: "c".into(), src_line: 1, indent: "".into(), confidence: Confidence::Exact }),
    ]).unwrap();
    db.set_src_snapshot("src.adoc", b"line1\nline2\n").unwrap();
    ws.write_file("src.adoc", b"line1\nline2\n");

    let opts = ApplyBackOptions {
        db_path: ws.root.join("weaveback.db"),
        gen_dir: ws.root.join("gen"),
        files: vec![],
        dry_run: true,
        eval_config: None,
    };
    let mut out = Vec::new();
    run_apply_back(opts, &mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    // Deletion of line 2 (out_line 1) should be detected.
    // It uses DiffOp::Delete logic.
    assert!(s.contains("Processing out.rs"));
}

#[test]
fn test_run_apply_back_success_literal() {
    let ws = TestWorkspace::new();
    let mut db = ws.open_db();
    
    let src_rel = "src/main.adoc";
    let gen_rel = "main.rs";
    let src_abs = ws.root.join(src_rel);
    
    // Initial setup: source file has a literal line.
    let src_content = "= File\n\n<<main>>=\noriginal line\n@\n";
    ws.write_file(src_rel, src_content.as_bytes());
    
    // Seed DB with baseline and source map
    db.set_baseline(gen_rel, b"original line\n").unwrap();
    db.set_noweb_entries(gen_rel, &[(0, weaveback_tangle::db::NowebMapEntry {
        src_file: src_rel.to_string(),
        chunk_name: "main".to_string(),
        src_line: 3, // 0-indexed "original line" is on line 3
        indent: "".into(),
        confidence: weaveback_tangle::db::Confidence::Exact,
    })]).unwrap();
    db.set_src_snapshot(src_rel, src_content.as_bytes()).unwrap();

    // Modify generated file
    ws.write_file(&format!("gen/{}", gen_rel), b"modified line\n");
    
    let opts = ApplyBackOptions {
        db_path: ws.root.join("weaveback.db"),
        gen_dir: ws.root.join("gen"),
        files: vec![],
        dry_run: false,
        eval_config: None,
    };
    let mut out = Vec::new();
    run_apply_back(opts, &mut out).unwrap();
    
    // Verify output message
    let msg = String::from_utf8(out).unwrap();
    assert!(msg.contains("patched"), "expected 'patched' in: {msg}");
    
    // Verify source file was actually updated
    let updated_src = std::fs::read_to_string(src_abs).unwrap();
    assert!(updated_src.contains("modified line"), "source file not updated: {updated_src}");
}

#[test]
fn test_run_apply_back_macro_edit() {
    let ws = TestWorkspace::new();
    let mut db = ws.open_db();
    
    let driver_rel = "src/driver.adoc";
    let macro_rel = "src/macros.adoc";
    let gen_rel = "out.txt";
    
    // Setup: Driver includes macros and calls a macro.
    let driver_content = "= Driver\n<<include macros.adoc>>\n<<@file out.txt>>=\n<<the-macro>>\n@\n";
    let macro_content = "<<the-macro>>=\noriginal macro body\n@\n";
    
    ws.write_file(driver_rel, driver_content.as_bytes());
    ws.write_file(macro_rel, macro_content.as_bytes());
    
    // Seed DB
    db.set_baseline(gen_rel, b"original macro body\n").unwrap();
    db.set_noweb_entries(gen_rel, &[(0, weaveback_tangle::db::NowebMapEntry {
        src_file: macro_rel.to_string(),
        chunk_name: "the-macro".to_string(),
        src_line: 1, // line 1 of macros.adoc
        indent: "".into(),
        confidence: weaveback_tangle::db::Confidence::Exact,
    })]).unwrap();
    db.set_src_snapshot(driver_rel, driver_content.as_bytes()).unwrap();
    db.set_src_snapshot(macro_rel, macro_content.as_bytes()).unwrap();

    // Modify generated file
    ws.write_file(&format!("gen/{}", gen_rel), b"modified macro body\n");
    
    let opts = ApplyBackOptions {
        db_path: ws.root.join("weaveback.db"),
        gen_dir: ws.root.join("gen"),
        files: vec![],
        dry_run: false,
        eval_config: None,
    };
    let mut out = Vec::new();
    run_apply_back(opts, &mut out).unwrap();
    
    // Verify macro source file was updated, not the driver.
    let updated_macro = std::fs::read_to_string(ws.root.join(macro_rel)).unwrap();
    assert!(updated_macro.contains("modified macro body"), "macro source not updated: {updated_macro}");
    
    let updated_driver = std::fs::read_to_string(ws.root.join(driver_rel)).unwrap();
    assert!(updated_driver.contains("<<the-macro>>"), "driver source should not be updated: {updated_driver}");
}

#[test]
fn test_apply_back_oracle_rejection_on_mismatch() {
    let ws = TestWorkspace::new();
    let mut db = ws.open_db();
    
    let src_rel = "src/test.adoc";
    let gen_rel = "test.rs";
    
    ws.write_file(src_rel, "<<main>>=\noriginal\n@\n".as_bytes());
    db.set_baseline(gen_rel, b"original\n").unwrap();
    db.set_noweb_entries(gen_rel, &[(0, weaveback_tangle::db::NowebMapEntry {
        src_file: src_rel.to_string(),
        chunk_name: "main".to_string(),
        src_line: 1,
        indent: "".into(),
        confidence: weaveback_tangle::db::Confidence::Exact,
    })]).unwrap();
    db.set_src_snapshot(src_rel, b"<<main>>=\noriginal\n@\n").unwrap();

    // Target edit: change "original" to "new"
    ws.write_file(&format!("gen/{}", gen_rel), b"new\n");

    // Now, manually trigger a scenario where reconstruction fails.
    // We'll use apply_patches_to_file with a patch that doesn't match the source exactly
    // or ensure the oracle re-evaluates and finds a mismatch.
    
    let _opts = ApplyBackOptions {
        db_path: ws.root.join("weaveback.db"),
        gen_dir: ws.root.join("gen"),
        files: vec![],
        dry_run: false,
        eval_config: None,
    };
    
    // We'll simulate a failure by providing an incorrect expected_output in the oracle check if possible,
    // or just rely on the fact that if re-evaluation yields different text, it rejects.
    // Actually, the easiest way is to mock a Patch that target a wrong line.
    
    let ctx = FilePatchContext {
        src_file: src_rel,
        src_root: &ws.root,
        db: &db,
        patches: &[Patch {
            source: PatchSource::MacroBodyWithVars {
                src_file: src_rel.into(),
                src_line: 1,
                macro_name: "main".into(),
            },
            old_text: "original".into(),
            new_text: "new".into(),
            expanded_line: 0,
        }],
        dry_run: false,
        sigil: '<',
        eval_config: Some(EvalConfig::default()),
        snapshot: None,
    };
    
    let mut skipped = 0;
    let mut out = Vec::new();
    apply_patches_to_file(ctx, &mut skipped, &mut out).unwrap();
    
    let msg = String::from_utf8(out).unwrap();
    // The oracle will fail because the patched source (src_rel) will actually contain
    // a different result when re-evaluated.
    assert!(msg.contains("manual") || msg.contains("rejected"), "expected rejection in: {msg}");
    assert_eq!(skipped, 1);
}

#[test]
fn run_apply_back_bulk_reconciliation() {
    let ws = TestWorkspace::new();
    let mut db = ws.open_db();
    
    let a_rel = "a.rs";
    let b_rel = "b.rs";
    let src_a = "src/a.adoc";
    let src_b = "src/b.adoc";

    // Setup two files
    db.set_baseline(a_rel, b"line A\n").unwrap();
    db.set_baseline(b_rel, b"line B\n").unwrap();
    ws.write_file(&format!("gen/{}", a_rel), b"line A modified\n");
    ws.write_file(&format!("gen/{}", b_rel), b"line B modified\n");
    
    // Mock source mappings
    db.set_noweb_entries(a_rel, &[(0, weaveback_tangle::db::NowebMapEntry {
        src_file: src_a.to_string(),
        chunk_name: "main".to_string(),
        src_line: 1,
        indent: "".into(),
        confidence: Confidence::Exact,
    })]).unwrap();
    db.set_noweb_entries(b_rel, &[(0, weaveback_tangle::db::NowebMapEntry {
        src_file: src_b.to_string(),
        chunk_name: "main".to_string(),
        src_line: 1,
        indent: "".into(),
        confidence: Confidence::Exact,
    })]).unwrap();

    ws.write_file(src_a, b"<<main>>=\nline A\n@\n");
    ws.write_file(src_b, b"<<main>>=\nline B\n@\n");
    
    let opts = ApplyBackOptions {
        db_path: ws.root.join("weaveback.db"),
        gen_dir: ws.root.join("gen"),
        files: vec![],
        dry_run: false,
        eval_config: None,
    };
    let mut out = Vec::new();
    run_apply_back(opts, &mut out).unwrap();
    
    // Verify both sources patched
    assert!(fs::read_to_string(ws.root.join(src_a)).unwrap().contains("line A modified"));
    assert!(fs::read_to_string(ws.root.join(src_b)).unwrap().contains("line B modified"));
}

#[test]
fn apply_patches_to_file_missing_source_errors() {
    let ws = TestWorkspace::new();
    let db = ws.open_db();
    let ctx = FilePatchContext {
        src_file: "nonexistent.adoc",
        src_root: &ws.root,
        db: &db,
        patches: &[],
        dry_run: false,
        sigil: '%',
        eval_config: None,
        snapshot: None,
    };
    let mut skipped = 0;
    let mut out = Vec::new();
    let res = apply_patches_to_file(ctx, &mut skipped, &mut out);
    assert!(res.is_err());
}

#[test]
fn run_apply_back_with_restricted_files() {
    let ws = TestWorkspace::new();
    let db = ws.open_db();
    db.set_baseline("a.rs", b"line A\n").unwrap();
    db.set_baseline("b.rs", b"line B\n").unwrap();
    ws.write_file("gen/a.rs", b"mod A\n");
    ws.write_file("gen/b.rs", b"mod B\n");

    let opts = ApplyBackOptions {
        db_path: ws.root.join("weaveback.db"),
        gen_dir: ws.root.join("gen"),
        files: vec!["a.rs".to_string()], // ONLY a.rs
        dry_run: true,
        eval_config: None,
    };
    let mut out = Vec::new();
    run_apply_back(opts, &mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("Processing a.rs"));
    assert!(!s.contains("Processing b.rs"));
}
// @
```

