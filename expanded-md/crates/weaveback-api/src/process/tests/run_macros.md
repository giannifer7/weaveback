# Single-Pass Macro Runner Tests

```rust
// <[@file weaveback-api/src/process/tests/run_macros.rs]>=
// weaveback-api/src/process/tests/run_macros.rs
// I'd Really Rather You Didn't edit this generated file.

use super::super::{run_single_pass, SinglePassArgs};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

// <[process-test-run-macros]>

// @
```


```rust
// <[process-test-run-macros]>=
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
// @
```

