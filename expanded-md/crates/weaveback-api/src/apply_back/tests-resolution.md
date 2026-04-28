# Apply-Back Resolution Tests

Patch-source resolution, LSP hints, and oracle-verified search behavior.

```rust
// <[applyback-tests-resolution]>=
// ── resolve_patch_source edge cases ─────────────────────────────────────

#[test]
fn resolve_patch_source_falls_back_to_noweb_if_trace_fails() {
    let ws = TestWorkspace::new();
    let db = ws.open_db();
    let config = EvalConfig { sigil: '%', ..Default::default() };
    let resolver = PathResolver::new(ws.root.clone(), ws.root.join("gen"));
    
    let patch_source = resolve_patch_source(
        "out.rs", 0, 0, &db, &resolver, &config,
        "src/file.adoc", 10, None, '%', 1
    ).unwrap();
    
    match patch_source {
        PatchSource::Noweb { src_file, src_line, len } => {
            assert_eq!(src_file, "src/file.adoc");
            assert_eq!(src_line, 10);
            assert_eq!(len, 1);
        }
        _ => panic!("Expected Noweb fallback"),
    }
}

#[test]
fn resolve_best_patch_source_falls_back_to_available_candidate() {
    let ws = TestWorkspace::new();
    let db = ws.open_db();
    let config = EvalConfig { sigil: '%', ..Default::default() };
    let resolver = PathResolver::new(ws.root.clone(), ws.root.join("gen"));
    
    let patch_source = resolve_best_patch_source(
        "out.rs", 0, "old", "new", 0, &db, &resolver, &config,
        "src/file.adoc", 10, None, '%', 1, None
    ).unwrap();
    
    match patch_source {
        PatchSource::Noweb { src_file, src_line, .. } => {
            assert_eq!(src_file, "src/file.adoc");
            assert_eq!(src_line, 10);
        }
        _ => panic!("Expected best source to be Noweb fallback"),
    }
}

// ── lsp_definition_hint edge case ───────────────────────────────────────

#[test]
fn lsp_definition_hint_returns_none_if_lsp_not_available() {
    let ws = TestWorkspace::new();
    let db = ws.open_db();
    let config = EvalConfig { sigil: '%', ..Default::default() };
    let resolver = PathResolver::new(ws.root.clone(), ws.root.join("gen"));
    let mut clients = std::collections::HashMap::new();

    let hint = lsp_definition_hint(
        "foo.unknown_extension", 0, 0, &resolver, &db, &config, &mut clients
    );
    assert!(hint.is_none());
}

// ── Oracle-verified searches ──────────────────────────────────────────

#[test]
fn test_search_macro_arg_candidate_verified() {
    let ws = TestWorkspace::new();
    let mut db = ws.open_db();
    db.set_chunk_defs(&[ChunkDefEntry {
        src_file: "src/file.adoc".into(),
        chunk_name: "foo".into(),
        nth: 0,
        def_start: 1,
        def_end: 5,
    }]).unwrap();

    let lines = lines("    let x = old;");
    let config = EvalConfig::default();
    let src_path = ws.root.join("src/file.adoc");
    
    let request = MacroArgSearch {
        db: &db,
        lines: &lines,
        hinted_line: 0,
        src_col: 12,
        old_text: "    let x = old;",
        new_text: "    let x = new;",
        eval_config: &config,
        src_path: &src_path,
        expanded_line: 0,
    };

    let best = search_macro_arg_candidate(request).unwrap();
    assert_eq!(best.line_idx, 0);
    assert_eq!(best.new_line, "    let x = new;");
    assert!(best.score >= 20, "expected bonus score, got {}", best.score);
}

#[test]
fn test_search_macro_body_candidate_verified() {
    let ws = TestWorkspace::new();
    let mut db = ws.open_db();
    db.set_chunk_defs(&[ChunkDefEntry {
        src_file: "src/file.adoc".into(),
        chunk_name: "foo".into(),
        nth: 0,
        def_start: 1,
        def_end: 5,
    }]).unwrap();

    let lines = lines("body line with old");
    let config = EvalConfig::default();
    let src_path = ws.root.join("src/file.adoc");

    let request = MacroBodySearch {
        db: &db,
        lines: &lines,
        hinted_line: 0,
        body_template: Some("body line with old"),
        old_text: "body line with old",
        new_text: "body line with new",
        sigil: '%',
        eval_config: &config,
        src_path: &src_path,
        expanded_line: 0,
    };

    let best = search_macro_body_candidate(request).unwrap();
    assert_eq!(best.line_idx, 0);
    assert_eq!(best.new_line, "body line with new");
}

#[test]
fn test_search_macro_call_candidate_verified() {
    let ws = TestWorkspace::new();
    let _db = ws.open_db();
    // Use a sigil '!' that differs from the default EvalConfig sigil '%'.
    // This ensures the oracle treats the patch as literal text and verification passes.
    let lines = lines("    !my_macro(val)");
    let config = EvalConfig::default(); // Uses '%'
    let src_path = ws.root.join("src/file.adoc");

    let request = MacroCallSearch {
        lines: &lines,
        macro_name: "my_macro",
        sigil: '!',
        old_text: "    !my_macro(val)",
        new_text: "    !my_macro(new_val)",
        eval_config: &config,
        src_path: &src_path,
        expanded_line: 0,
    };

    let best = search_macro_call_candidate(request).unwrap();
    assert_eq!(best.line_idx, 0);
    assert_eq!(best.new_line, "    !my_macro(new_val)");
}
// @
```

