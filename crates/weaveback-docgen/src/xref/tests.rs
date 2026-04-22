// weaveback-docgen/src/xref/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn module_and_html_keys_roundtrip_basic_workspace_paths() {
    let crates_dir = Path::new("/tmp/ws/crates");
    let rs = crates_dir.join("weaveback-tangle/src/noweb.rs");
    assert_eq!(module_key(&rs, crates_dir).as_deref(), Some("weaveback_tangle/noweb"));
    assert_eq!(html_path_for_key("weaveback_tangle/noweb"), "crates/weaveback-tangle/src/noweb.html");
    assert_eq!(html_path_for_key("index"), "index.html");
}

#[test]
fn collect_and_resolve_imports_cover_common_rust_forms() {
    let dir = tempdir().expect("tempdir");
    let crates_dir = dir.path().join("crates");
    fs::create_dir_all(crates_dir.join("demo/src/parser")).expect("parser dir");
    fs::write(
        crates_dir.join("demo/Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .expect("cargo");
    fs::write(crates_dir.join("demo/src/lib.rs"), "pub mod parser;\n").expect("lib");
    fs::write(crates_dir.join("demo/src/parser/mod.rs"), "pub fn parse() {}\n").expect("parser");

    let known = workspace_crate_names(&crates_dir);
    assert_eq!(known, vec!["demo".to_string()]);

    let demo_file = crates_dir.join("demo/src/demo.rs");
    fs::write(
        &demo_file,
        "pub use crate::parser::parse;\nuse crate::{parser::parse as parse2};\npub struct Demo;\n",
    )
    .expect("demo");

    let (uses, symbols) = analyze_file(&demo_file);
    assert!(uses.iter().any(|u| u == "crate::parser::parse"));
    assert!(symbols.iter().any(|s| s == "Demo"));

    assert_eq!(
        resolve_to_module(&["parser", "parse"], &crates_dir.join("demo"), "demo").as_deref(),
        Some("demo/parser")
    );
    assert_eq!(
        resolve_import("crate::parser::parse", "demo/demo", "demo", &crates_dir, &known).as_deref(),
        Some("demo/parser")
    );
    assert_eq!(
        resolve_import("super::parser::parse", "demo/nested/mod", "demo", &crates_dir, &known).as_deref(),
        None
    );
}

#[test]
fn collect_use_tree_and_collect_items_cover_groups_globs_and_pub_items() {
    let tree: syn::UseTree = syn::parse_str("crate::{alpha::Beta, gamma::*, delta as renamed}").expect("use tree");
    let mut out = Vec::new();
    collect_use_tree(&tree, "", &mut out);
    assert_eq!(
        out,
        vec![
            "crate::alpha::Beta".to_string(),
            "crate::gamma".to_string(),
            "crate::delta".to_string(),
        ]
    );

    let file = syn::parse_file(
        "pub fn hello() {}\nstruct Hidden;\npub mod inner { pub struct Visible; }\nuse crate::alpha::Beta;\n",
    )
    .expect("file");
    let mut use_paths = Vec::new();
    let mut symbols = Vec::new();
    collect_items(&file.items, &mut use_paths, &mut symbols);
    assert!(use_paths.iter().any(|u| u == "crate::alpha::Beta"));
    assert!(symbols.iter().any(|s| s == "hello"));
    assert!(symbols.iter().any(|s| s == "inner"));
    assert!(symbols.iter().any(|s| s == "Visible"));
    assert!(!symbols.iter().any(|s| s == "Hidden"));
}

#[test]
fn adoc_scan_and_line_col_helpers_work() {
    let dir = tempdir().expect("tempdir");
    let project_root = dir.path();
    let crates_dir = project_root.join("crates");
    fs::create_dir_all(crates_dir.join("demo/src")).expect("src dir");
    fs::write(
        crates_dir.join("demo/Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .expect("cargo");
    fs::write(crates_dir.join("demo/src/lib.rs"), "pub fn hello() {}\n").expect("lib");
    fs::write(
        crates_dir.join("demo/src/lib.adoc"),
        "// <<@file demo/src/lib.rs>>=\nbody\n// @\n",
    )
    .expect("adoc");

    let map = scan_adoc_file_declarations(project_root, &crates_dir);
    assert_eq!(
        map.get("crates/demo/src/lib.html"),
        Some(&vec!["demo/lib".to_string()])
    );
    assert_eq!(find_line_col("ab\ncde", 0), (1, 1));
    assert_eq!(find_line_col("ab\ncde", 3), (2, 1));
    assert_eq!(find_line_col("ab\ncde", 5), (2, 3));
}

#[test]
fn is_excluded_matches_expected_workspace_noise_dirs() {
    assert!(is_excluded(Path::new("/tmp/project/target/file.rs")));
    assert!(is_excluded(Path::new("/tmp/project/.git/config")));
    assert!(is_excluded(Path::new("/tmp/project/node_modules/pkg/index.js")));
    assert!(!is_excluded(Path::new("/tmp/project/crates/demo/src/lib.rs")));
}

#[test]
fn test_build_xref_orchestration() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    let crates_dir = root.join("crates");

    let crate_a = crates_dir.join("crate-a");
    fs::create_dir_all(crate_a.join("src")).unwrap();
    fs::write(crate_a.join("Cargo.toml"), "[package]\nname = \"crate-a\"\n").unwrap();
    fs::write(crate_a.join("src/lib.rs"), "pub mod sub;").unwrap();
    fs::write(crate_a.join("src/sub.rs"), "pub struct Alpha;").unwrap();

    let crate_b = crates_dir.join("crate-b");
    fs::create_dir_all(crate_b.join("src")).unwrap();
    fs::write(crate_b.join("Cargo.toml"), "[package]\nname = \"crate-b\"\n").unwrap();
    fs::write(crate_b.join("src/lib.rs"), "use crate_a::sub::Alpha; pub struct Beta;").unwrap();

    let xref = build_xref(root, false);
    assert!(xref.contains_key("crate_a/sub"));
    assert!(xref.contains_key("crate_b/lib"));

    let a = xref.get("crate_a/sub").unwrap();
    assert!(a.symbols.contains(&"Alpha".to_string()));
    assert!(a.imported_by.iter().any(|l| l.key == "crate_b/lib"));

    let b = xref.get("crate_b/lib").unwrap();
    assert!(b.imports.iter().any(|l| l.key == "crate_a/sub"));
}

