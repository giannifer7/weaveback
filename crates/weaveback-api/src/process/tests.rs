// weaveback-api/src/process/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn find_files_discovers_matching_extensions() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("a.adoc"), b"").unwrap();
    fs::write(tmp.path().join("b.txt"), b"").unwrap();
    let sub = tmp.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("c.adoc"), b"").unwrap();

    let mut out = Vec::new();
    find_files(tmp.path(), &["adoc".to_string()], &mut out).unwrap();
    out.sort();
    assert_eq!(out.len(), 2);
    assert!(out[0].ends_with("a.adoc") || out[1].ends_with("a.adoc"));
    assert!(out.iter().any(|p| p.ends_with("c.adoc")));
    assert!(!out.iter().any(|p| p.ends_with("b.txt")));
}

#[test]
fn find_files_returns_empty_for_no_match() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("x.txt"), b"").unwrap();
    let mut out = Vec::new();
    find_files(tmp.path(), &["adoc".to_string()], &mut out).unwrap();
    assert!(out.is_empty());
}

#[test]
fn write_depfile_produces_makefile_format() {
    let tmp = tempdir().unwrap();
    let dep_path = tmp.path().join("out.d");
    let target = std::path::Path::new("out.rs");
    let deps = vec![
        PathBuf::from("src/a.adoc"),
        PathBuf::from("src/b.adoc"),
    ];
    write_depfile(&dep_path, target, &deps).unwrap();
    let content = fs::read_to_string(&dep_path).unwrap();
    assert!(content.starts_with("out.rs:"));
    assert!(content.contains("src/a.adoc"));
    assert!(content.contains("src/b.adoc"));
    assert!(content.ends_with('\n'));
}

#[test]
fn write_depfile_escapes_spaces_in_paths() {
    let tmp = tempdir().unwrap();
    let dep_path = tmp.path().join("out.d");
    let target = std::path::Path::new("my out.rs");
    let deps = vec![PathBuf::from("src/my file.adoc")];
    write_depfile(&dep_path, target, &deps).unwrap();
    let content = fs::read_to_string(&dep_path).unwrap();
    assert!(content.contains(r"my\ out.rs"));
    assert!(content.contains(r"my\ file.adoc"));
}

#[test]
fn normalize_adoc_table_to_markdown_pipe_table() {
    let input = concat!(
        "[cols=\"1,2\",options=\"header\"]\n",
        "|===\n",
        "| Name | Meaning\n",
        "\n",
        "| `%def` | Constant binding\n",
        "| `%redef` | Rebindable binding\n",
        "|===\n",
    );

    let out = normalize_adoc_tables_for_markdown(input);
    assert_eq!(
        out,
        concat!(
            "| Name | Meaning |\n",
            "| --- | --- |\n",
            "| `%def` | Constant binding |\n",
            "| `%redef` | Rebindable binding |\n",
        )
    );
}

#[test]
fn normalize_adoc_table_handles_split_rows() {
    let input = concat!(
        "[cols=\"2,1,4\",options=\"header\"]\n",
        "|===\n",
        "| Path | Method | Description\n",
        "\n",
        "| `/__events` | GET\n",
        "| SSE stream.\n",
        "| `/__open` | GET\n",
        "| Opens an editor.\n",
        "|===\n",
    );

    let out = normalize_adoc_tables_for_markdown(input);
    assert!(out.contains("| `/__events` | GET | SSE stream. |"), "out: {out}");
    assert!(out.contains("| `/__open` | GET | Opens an editor. |"), "out: {out}");
}

#[test]
fn normalize_adoc_table_uses_html_for_complex_cells() {
    let input = concat!(
        "[cols=\"1,2\",options=\"header\"]\n",
        "|===\n",
        "| Error | Meaning\n",
        "| `UndefinedChunk`\n",
        "| A reference names a chunk that was never defined. Silently expands to\n",
        "  nothing by default.\n",
        "|===\n",
    );

    let out = normalize_adoc_tables_for_markdown(input);
    assert!(out.starts_with("<table>"), "out: {out}");
    assert!(out.contains("<br>"), "out: {out}");
    assert!(out.contains("nothing by default."), "out: {out}");
}

#[test]
fn normalize_adoc_table_skips_fenced_code_blocks() {
    let input = concat!(
        "```text\n",
        "[cols=\"1,1\",options=\"header\"]\n",
        "|===\n",
        "| A | B\n",
        "|===\n",
        "```\n",
    );

    assert_eq!(normalize_adoc_tables_for_markdown(input), input);
}

#[test]
fn compute_skip_set_with_no_prev_db_returns_empty() {
    let mut current_db = weaveback_tangle::db::WeavebackDb::open_temp().unwrap();
    let sources: HashMap<String, String> = HashMap::new();
    let skip = compute_skip_set(&sources, &None, &mut current_db, std::path::Path::new("/tmp"));
    assert!(skip.is_empty());
}

#[test]
fn run_single_pass_writes_output_file() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("input.adoc");
    let gen_dir = tmp.path().join("gen_out");
    fs::create_dir(&gen_dir).unwrap();

    fs::write(&src, "// <<@file output.txt>>=\nhello world\n// @\n").unwrap();

    let db_path = tmp.path().join("wb.db");
    let args = SinglePassArgs {
        inputs: vec![src.file_name().unwrap().into()],
        directory: None,
        input_dir: tmp.path().to_path_buf(),
        gen_dir: gen_dir.clone(),
        open_delim: "<<".to_string(),
        close_delim: ">>".to_string(),
        chunk_end: "@".to_string(),
        comment_markers: "//,#".to_string(),
        ext: vec!["adoc".to_string()],
        no_macros: true,
        macro_prelude: vec![],
        expanded_ext: None,
        expanded_adoc_dir: PathBuf::from("expanded-adoc"),
        expanded_md_dir: PathBuf::from("expanded-md"),
        macro_only: false,
        dry_run: false,
        db: db_path,
        depfile: None,
        stamp: None,
        strict: false,
        warn_unused: false,
        allow_env: false,
        allow_home: true,
        force_generated: false,
        sigil: '%',
        include: String::new(),
        formatter: vec![],
        no_fts: true,
        dump_expanded: false,
        project_root: None,
    };
    run_single_pass(args).unwrap();
    let out = fs::read_to_string(gen_dir.join("output.txt")).unwrap();
    assert_eq!(out.trim(), "hello world");
}

#[test]
fn test_run_single_pass_force_generated() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("input.adoc");
    let gen_dir = tmp.path().join("gen_out");
    fs::create_dir(&gen_dir).unwrap();

    fs::write(&src, "// <<@file output.txt>>=\ncontent\n// @\n").unwrap();
    let out_file = gen_dir.join("output.txt");
    fs::write(&out_file, "old").unwrap();

    let db_path = tmp.path().join("wb.db");
    let args = SinglePassArgs {
        inputs: vec![src.file_name().unwrap().into()],
        directory: None,
        input_dir: tmp.path().to_path_buf(),
        gen_dir: gen_dir.clone(),
        open_delim: "<<".to_string(),
        close_delim: ">>".to_string(),
        chunk_end: "@".to_string(),
        comment_markers: "//,#".to_string(),
        ext: vec!["adoc".to_string()],
        no_macros: true,
        macro_prelude: vec![],
        expanded_ext: None,
        expanded_adoc_dir: PathBuf::from("expanded-adoc"),
        expanded_md_dir: PathBuf::from("expanded-md"),
        macro_only: false,
        dry_run: false,
        db: db_path,
        depfile: None,
        stamp: None,
        strict: false,
        warn_unused: false,
        allow_env: false,
        allow_home: true,
        force_generated: true, // Force!
        sigil: '%',
        include: String::new(),
        formatter: vec![],
        no_fts: true,
        dump_expanded: false,
        project_root: None,
    };
    run_single_pass(args).unwrap();
    assert_eq!(fs::read_to_string(&out_file).unwrap().trim(), "content");

    // force_generated is for when it *matches* the baseline but we want to write anyway.
}

#[test]
fn run_single_pass_respects_include_paths() {
    let tmp = tempdir().unwrap();
    let lib_dir = tmp.path().join("lib");
    fs::create_dir(&lib_dir).unwrap();
    fs::write(lib_dir.join("macros.adoc"), "%set(test_macro, library content)\n").unwrap();

    let src = tmp.path().join("input.adoc");
    fs::write(&src, "%include(macros.adoc)\n<<@file output.txt>>=\n%(test_macro)\n@\n").unwrap();

    let gen_dir = tmp.path().join("gen_out");
    fs::create_dir(&gen_dir).unwrap();

    let args = SinglePassArgs {
        inputs: vec![src.file_name().unwrap().into()],
        input_dir: tmp.path().to_path_buf(),
        include: lib_dir.to_string_lossy().into_owned(),
        gen_dir: gen_dir.clone(),
        db: tmp.path().join("wb.db"),
        no_fts: true,
        no_macros: false,
        ..SinglePassArgs::default_for_test()
    };
    run_single_pass(args).unwrap();

    let out = fs::read_to_string(gen_dir.join("output.txt")).unwrap();
    assert!(out.contains("library content"), "Output should contain 'library content', got: '{}'", out);
}

#[test]
fn run_single_pass_import_change_invalidates_incremental_output() {
    let tmp = tempdir().unwrap();
    let lib_dir = tmp.path().join("lib");
    fs::create_dir(&lib_dir).unwrap();
    let macros = lib_dir.join("macros.adoc");
    fs::write(&macros, "%set(test_macro, first)\n").unwrap();

    let src = tmp.path().join("input.adoc");
    fs::write(&src, "%include(macros.adoc)\n<<@file output.txt>>=\n%(test_macro)\n@\n").unwrap();

    let gen_dir = tmp.path().join("gen_out");
    fs::create_dir(&gen_dir).unwrap();
    let db = tmp.path().join("wb.db");

    let make_args = || SinglePassArgs {
        inputs: vec![src.file_name().unwrap().into()],
        input_dir: tmp.path().to_path_buf(),
        include: lib_dir.to_string_lossy().into_owned(),
        gen_dir: gen_dir.clone(),
        db: db.clone(),
        no_fts: true,
        no_macros: false,
        force_generated: true,
        ..SinglePassArgs::default_for_test()
    };

    run_single_pass(make_args()).unwrap();
    assert_eq!(fs::read_to_string(gen_dir.join("output.txt")).unwrap().trim(), "first");

    fs::write(&macros, "%set(test_macro, second)\n").unwrap();
    run_single_pass(make_args()).unwrap();
    assert_eq!(fs::read_to_string(gen_dir.join("output.txt")).unwrap().trim(), "second");
}

#[test]
fn run_single_pass_uses_macro_prelude_and_expanded_ext() {
    let tmp = tempdir().unwrap();
    let prelude = tmp.path().join("asciidoc.wvb");
    fs::write(&prelude, concat!(r#"
¤redef(rust_file, path, body, ¤{
[source,rust]
----
"#, "// ", r#"<[@file ¤(path)]>=
¤(body)
"#, "// ", "@", r#"
----
¤})
"#)).unwrap();

    let src = tmp.path().join("input.wvb");
    fs::write(&src, concat!("¤", "rust_file(output.rs, ", "¤", "[pub fn answer() -> u8 { 42 }\n", "¤", "])")).unwrap();

    let gen_dir = tmp.path().join("gen_out");
    fs::create_dir(&gen_dir).unwrap();
    let expanded_adoc_dir = tmp.path().join("expanded-adoc-out");
    let expanded_md_dir = tmp.path().join("expanded-md-out");

    let args = SinglePassArgs {
        inputs: vec![src.file_name().unwrap().into()],
        input_dir: tmp.path().to_path_buf(),
        gen_dir: gen_dir.clone(),
        db: tmp.path().join("wb.db"),
        no_fts: true,
        no_macros: false,
        macro_prelude: vec![prelude],
        expanded_ext: Some("adoc".to_string()),
        sigil: '¤',
        open_delim: "<[".to_string(),
        close_delim: "]>".to_string(),
        comment_markers: "//".to_string(),
        expanded_adoc_dir: expanded_adoc_dir.clone(),
        expanded_md_dir: expanded_md_dir.clone(),
        ..SinglePassArgs::default_for_test()
    };
    run_single_pass(args).unwrap();

    let out = fs::read_to_string(gen_dir.join("output.rs")).unwrap();
    assert!(out.contains("pub fn answer() -> u8 { 42 }"));
    let expanded = fs::read_to_string(expanded_adoc_dir.join("input.adoc")).unwrap();
    assert!(expanded.contains("<[@file output.rs]>"));
    assert!(!expanded_md_dir.join("input.adoc").exists());
}

#[test]
fn adoc_and_markdown_preludes_tangle_to_same_output() {
    let tmp = tempdir().unwrap();
    let adoc_prelude = tmp.path().join("asciidoc.wvb");
    fs::write(&adoc_prelude, concat!(r#"
¤redef(rust_file, path, body, ¤{
[source,rust]
----
"#, "// ", r#"<[@file ¤(path)]>=
¤(body)
"#, "// ", "@", r#"
----
¤})
"#)).unwrap();

    let md_prelude = tmp.path().join("markdown.wvb");
    fs::write(&md_prelude, concat!(r#"
¤redef(rust_file, path, body, ¤{
```rust
"#, "// ", r#"<[@file ¤(path)]>=
¤(body)
"#, "// ", "@", r#"
```
¤})
"#)).unwrap();

    let src = tmp.path().join("input.wvb");
    fs::write(&src, concat!("¤", "rust_file(output.rs, ", "¤", "[pub fn answer() -> u8 {\n    42\n}\n", "¤", "])")).unwrap();

    let adoc_gen = tmp.path().join("gen-adoc");
    let md_gen = tmp.path().join("gen-md");
    let expanded_adoc_dir = tmp.path().join("expanded-adoc");
    let expanded_md_dir = tmp.path().join("expanded-md");

    let common = |gen_dir: PathBuf, db_name: &str, prelude: PathBuf, ext: &str| SinglePassArgs {
        inputs: vec![src.file_name().unwrap().into()],
        input_dir: tmp.path().to_path_buf(),
        gen_dir,
        expanded_adoc_dir: expanded_adoc_dir.clone(),
        expanded_md_dir: expanded_md_dir.clone(),
        db: tmp.path().join(db_name),
        no_fts: true,
        no_macros: false,
        macro_prelude: vec![prelude],
        expanded_ext: Some(ext.to_string()),
        sigil: '¤',
        open_delim: "<[".to_string(),
        close_delim: "]>".to_string(),
        comment_markers: "//".to_string(),
        ..SinglePassArgs::default_for_test()
    };

    run_single_pass(common(adoc_gen.clone(), "adoc.db", adoc_prelude, "adoc")).unwrap();
    run_single_pass(common(md_gen.clone(), "md.db", md_prelude, "md")).unwrap();

    let adoc_out = fs::read_to_string(adoc_gen.join("output.rs")).unwrap();
    let md_out = fs::read_to_string(md_gen.join("output.rs")).unwrap();
    assert_eq!(adoc_out, md_out);
    assert!(expanded_adoc_dir.join("input.adoc").exists());
    assert!(expanded_md_dir.join("input.md").exists());
}

#[test]
fn run_single_pass_macro_only_writes_expanded_documents() {
    let tmp = tempdir().unwrap();
    let prelude = tmp.path().join("asciidoc.wvb");
    fs::write(&prelude, "¤redef(doc, body, ¤{= Generated\n\n¤(body)¤})").unwrap();

    let src = tmp.path().join("input.wvb");
    fs::write(&src, "¤doc(hello)").unwrap();

    let gen_dir = tmp.path().join("gen");
    let expanded_adoc_dir = tmp.path().join("expanded-adoc");
    let expanded_md_dir = tmp.path().join("expanded-md");
    fs::create_dir(&gen_dir).unwrap();
    fs::create_dir(&expanded_adoc_dir).unwrap();
    fs::create_dir(&expanded_md_dir).unwrap();

    let args = SinglePassArgs {
        inputs: vec![src.file_name().unwrap().into()],
        input_dir: tmp.path().to_path_buf(),
        gen_dir: gen_dir.clone(),
        expanded_adoc_dir: expanded_adoc_dir.clone(),
        expanded_md_dir: expanded_md_dir.clone(),
        db: tmp.path().join("wb.db"),
        no_fts: true,
        no_macros: false,
        macro_prelude: vec![prelude],
        expanded_ext: Some("adoc".to_string()),
        macro_only: true,
        sigil: '¤',
        ..SinglePassArgs::default_for_test()
    };
    run_single_pass(args).unwrap();

    let expanded = fs::read_to_string(expanded_adoc_dir.join("input.adoc")).unwrap();
    assert!(expanded.contains("= Generated"));
    assert!(expanded.contains("hello"));
    assert!(!gen_dir.join("input.adoc").exists());
    assert!(!expanded_md_dir.join("input.adoc").exists());
}


#[test]
fn run_single_pass_normalizes_adoc_tables_in_markdown_expanded_documents() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("input.wvb");
    fs::write(&src, concat!(
        "[cols=\"1,1\",options=\"header\"]\n",
        "|===\n",
        "| A | B\n",
        "| one | two\n",
        "|===\n",
    )).unwrap();

    let gen_dir = tmp.path().join("gen");
    let expanded_adoc_dir = tmp.path().join("expanded-adoc");
    let expanded_md_dir = tmp.path().join("expanded-md");

    let args = SinglePassArgs {
        inputs: vec![src.file_name().unwrap().into()],
        input_dir: tmp.path().to_path_buf(),
        gen_dir,
        expanded_adoc_dir: expanded_adoc_dir.clone(),
        expanded_md_dir: expanded_md_dir.clone(),
        db: tmp.path().join("wb.db"),
        no_fts: true,
        no_macros: false,
        expanded_ext: Some("md".to_string()),
        macro_only: true,
        sigil: '¤',
        ..SinglePassArgs::default_for_test()
    };
    run_single_pass(args).unwrap();

    let expanded = fs::read_to_string(expanded_md_dir.join("input.md")).unwrap();
    assert!(expanded.contains("| A | B |"), "expanded: {expanded}");
    assert!(!expanded.contains("|==="), "expanded: {expanded}");
    assert!(!expanded_adoc_dir.join("input.md").exists());
}


    #[test]
fn run_single_pass_with_custom_sigil() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("input.adoc");
    // Use '%' instead of default '<<'
    fs::write(&src, "%@file output.txt%=\ncontent\n%@\n").unwrap();

    let gen_dir = tmp.path().join("gen_out");
    fs::create_dir(&gen_dir).unwrap();

    let db_path = tmp.path().join("wb.db");
    let args = SinglePassArgs {
        inputs: vec![src.file_name().unwrap().into()],
        input_dir: tmp.path().to_path_buf(),
        gen_dir: gen_dir.clone(),
        sigil: '%',
        open_delim: "%".into(),
        close_delim: "%".into(),
        chunk_end: "%@".into(),
        db: db_path,
        no_fts: true,
        ..SinglePassArgs::default_for_test()
    };
    run_single_pass(args).unwrap();

    let out = fs::read_to_string(gen_dir.join("output.txt")).unwrap();
    assert_eq!(out.trim(), "content");
}

#[test]
fn compute_skip_set_propagates_dirty_chunks() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("prev.db");

    let mut prev_db = WeavebackDb::open(&db_path).unwrap();
    // Chunk B depends on Chunk A
    prev_db.set_chunk_defs(&[
        weaveback_tangle::db::ChunkDefEntry {
            src_file: "src.adoc".into(),
            chunk_name: "A".into(),
            nth: 0,
            def_start: 1,
            def_end: 10,
        },
        weaveback_tangle::db::ChunkDefEntry {
            src_file: "src.adoc".into(),
            chunk_name: "B".into(),
            nth: 0,
            def_start: 11,
            def_end: 20,
        },
    ]).unwrap();
    prev_db.set_chunk_deps(&[("B".into(), "A".into(), "src.adoc".into())]).unwrap();

    // Block 0 covers lines 1-10 (Chunk A)
    let block_a = weaveback_tangle::block_parser::SourceBlockEntry {
        block_index: 0,
        block_type: "code".into(),
        line_start: 1,
        line_end: 10,
        content_hash: [0u8; 32],
    };
    prev_db.set_source_blocks("src.adoc", std::slice::from_ref(&block_a)).unwrap();

    let mut current_db = WeavebackDb::open_temp().unwrap();
    let mut source_contents = HashMap::new();
    // Use content that will trigger the same block index but different hash
    // We'll mock the blocks directly because compute_skip_set calls parse_source_blocks
    // which we can't easily mock across crates without real content.
    source_contents.insert("src.adoc".to_string(), "<<A>>=\nnew content\n@".to_string());

    let skip = compute_skip_set(&source_contents, &Some(prev_db), &mut current_db, tmp.path());

    // Since original blocks were [1,2,3] and new will be different,
    // Chunk A becomes dirty, and Chunk B becomes dirty via reverse deps.
    assert!(!skip.contains("A"));
    assert!(!skip.contains("B"));
}

#[test]
fn run_single_pass_directory_mode() {
    let tmp = tempdir().unwrap();
    let src_a = tmp.path().join("a.adoc");
    let src_b = tmp.path().join("b.adoc");
    fs::write(&src_a, "<<@file a.txt>>=\nA\n@").unwrap();
    fs::write(&src_b, "<<@file b.txt>>=\nB\n@").unwrap();

    let gen_dir = tmp.path().join("gen");
    fs::create_dir(&gen_dir).unwrap();

    let args = SinglePassArgs {
        directory: Some(tmp.path().to_path_buf()),
        ext: vec!["adoc".to_string()],
        gen_dir: gen_dir.clone(),
        db: tmp.path().join("wb.db"),
        no_fts: true,
        ..SinglePassArgs::default_for_test()
    };
    run_single_pass(args).expect("run_single_pass failed");

    assert_eq!(fs::read_to_string(gen_dir.join("a.txt")).unwrap().trim(), "A");
    assert_eq!(fs::read_to_string(gen_dir.join("b.txt")).unwrap().trim(), "B");
}

#[test]
fn run_single_pass_depfile_and_stamp() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("input.adoc");
    fs::write(&src, "<<@file out.txt>>=\ncontent\n@").unwrap();

    let gen_dir = tmp.path().join("gen");
    fs::create_dir(&gen_dir).unwrap();
    let depfile = tmp.path().join("out.d");
    let stamp = tmp.path().join("out.stamp");

    let args = SinglePassArgs {
        inputs: vec![PathBuf::from("input.adoc")],
        input_dir: tmp.path().to_path_buf(),
        gen_dir,
        db: tmp.path().join("wb.db"),
        depfile: Some(depfile.clone()),
        stamp: Some(stamp.clone()),
        no_fts: true,
        ..SinglePassArgs::default_for_test()
    };
    run_single_pass(args).unwrap();

    assert!(depfile.exists());
    assert!(stamp.exists());
    let dep_content = fs::read_to_string(depfile).unwrap();
    assert!(dep_content.contains("input.adoc"));
}

#[test]
fn run_single_pass_dry_run_no_writes() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("input.adoc");
    fs::write(&src, "<<@file out.txt>>=\ncontent\n@").unwrap();

    let gen_dir = tmp.path().join("gen");
    fs::create_dir(&gen_dir).unwrap();

    let args = SinglePassArgs {
        inputs: vec![PathBuf::from("input.adoc")],
        input_dir: tmp.path().to_path_buf(),
        gen_dir: gen_dir.clone(),
        db: tmp.path().join("wb.db"),
        dry_run: true,
        no_fts: true,
        ..SinglePassArgs::default_for_test()
    };
    run_single_pass(args).unwrap();

    assert!(!gen_dir.join("out.txt").exists(), "Dry run should not write files");
}

#[test]
fn run_single_pass_error_missing_input() {
    let tmp = tempdir().unwrap();
    let args = SinglePassArgs {
        inputs: vec![PathBuf::from("missing.adoc")],
        input_dir: tmp.path().to_path_buf(),
        gen_dir: tmp.path().join("gen"),
        db: tmp.path().join("wb.db"),
        no_fts: true,
        ..SinglePassArgs::default_for_test()
    };
    let res = run_single_pass(args);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.contains("No such file or directory") || err.contains("not found"));
}

#[test]
fn run_single_pass_with_macro_expansion() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("input.adoc");
    fs::write(&src, "%set(V, expanded)\n<<@file out.txt>>=\n%(V)\n@").unwrap();

    let gen_dir = tmp.path().join("gen");
    fs::create_dir(&gen_dir).unwrap();

    let args = SinglePassArgs {
        inputs: vec![PathBuf::from("input.adoc")],
        input_dir: tmp.path().to_path_buf(),
        gen_dir: gen_dir.clone(),
        db: tmp.path().join("wb.db"),
        no_macros: false, // Enable macros!
        no_fts: true,
        ..SinglePassArgs::default_for_test()
    };
    run_single_pass(args).expect("run_single_pass failed with macros");
    let out = fs::read_to_string(gen_dir.join("out.txt")).unwrap();
    assert_eq!(out.trim(), "expanded");
}

#[test]
fn run_single_pass_with_var_defs_recording() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("input.adoc");
    fs::write(&src, "%set(MYVAR, value)\n<<@file out.txt>>=\n%(MYVAR)\n@").unwrap();

    let db_path = tmp.path().join("wb.db");
    let args = SinglePassArgs {
        inputs: vec![PathBuf::from("input.adoc")],
        input_dir: tmp.path().to_path_buf(),
        gen_dir: tmp.path().join("gen"),
        db: db_path.clone(),
        no_macros: false,
        no_fts: true,
        ..SinglePassArgs::default_for_test()
    };
    run_single_pass(args).unwrap();

    let db = weaveback_tangle::db::WeavebackDb::open(&db_path).unwrap();
    let vars = db.query_var_defs("MYVAR").unwrap();
    assert!(!vars.is_empty(), "Should have recorded MYVAR definition");
    assert!(vars[0].0.contains("input.adoc"));
}

#[test]
fn find_files_error_on_missing_dir() {
    let res = find_files(std::path::Path::new("/non/existent/path/for/weaveback/test"), &["adoc".to_string()], &mut Vec::new());
    assert!(res.is_err());
}

#[test]
fn test_compute_skip_set_with_dependencies() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("wb.db");
    let mut db = weaveback_tangle::db::WeavebackDb::open(&db_path).unwrap();

    let path = "test.adoc";
    let content = "base content";
    let blocks = weaveback_tangle::parse_source_blocks(content, "adoc");
    db.set_source_blocks(path, &blocks).unwrap();
    db.set_chunk_defs(&[weaveback_tangle::db::ChunkDefEntry {
        src_file: path.to_string(),
        chunk_name: "base".to_string(),
        nth: 0,
        def_start: 1,
        def_end: 1,
    }]).unwrap();
    db.set_chunk_deps(&[("dep".to_string(), "base".to_string(), path.to_string())]).unwrap();

    let mut source_contents = HashMap::new();
    source_contents.insert(path.to_string(), "changed content".to_string());

    let mut current_db = weaveback_tangle::db::WeavebackDb::open_temp().unwrap();
    let skip_set = compute_skip_set(&source_contents, &Some(db), &mut current_db, tmp.path());

    // "base" is dirty because content changed.
    // "dep" should be dirty via reverse dependency.
    assert!(!skip_set.contains("base"));
    assert!(!skip_set.contains("dep"));
}

#[test]
fn run_single_pass_bench_no_fts() {
    // Just verify it doesn't crash when no_fts is false and db is present
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("input.adoc");
    fs::write(&src, "<<@file out.txt>>=\n@").unwrap();
    let args = SinglePassArgs {
        inputs: vec![PathBuf::from("input.adoc")],
        input_dir: tmp.path().to_path_buf(),
        gen_dir: tmp.path().join("gen"),
        db: tmp.path().join("wb.db"),
        no_fts: false, // Rebuild FTS!
        ..SinglePassArgs::default_for_test()
    };
    run_single_pass(args).unwrap();
}

