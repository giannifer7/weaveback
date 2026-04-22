// wb-tangle/src/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use std::path::PathBuf;

struct TestWorkspace {
    root: PathBuf,
}

impl TestWorkspace {
    fn new() -> Self {
        let unique = format!(
            "wb-tangle-tests-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn db(&self) -> PathBuf {
        self.root.join("weaveback.db")
    }

    fn gen_dir(&self) -> PathBuf {
        self.root.join("gen")
    }

    fn gen_file(&self, path: &str) -> PathBuf {
        self.gen_dir().join(path)
    }

    fn open_db(&self) -> weaveback_tangle::db::WeavebackDb {
        weaveback_tangle::db::WeavebackDb::open(self.db()).unwrap()
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn default_single_pass(root: &TestWorkspace) -> SinglePassCli {
    SinglePassCli {
        inputs: vec![],
        input_dir: PathBuf::from("."),
        sigil: '%',
        no_macros: false,
        macro_prelude: vec![],
        expanded_ext: None,
        expanded_adoc_dir: root.root.join("expanded-adoc"),
        expanded_md_dir: root.root.join("expanded-md"),
        macro_only: false,
        include: ".".to_string(),
        db: root.db(),
        dump_expanded: false,
        directory: None,
        ext: vec!["adoc".to_string()],
        gen_dir: root.gen_dir(),
        open_delim: "<[".to_string(),
        close_delim: "]>".to_string(),
        chunk_end: "@".to_string(),
        comment_markers: "#,//".to_string(),
        formatter: vec![],
        depfile: None,
        stamp: None,
        no_fts: false,
        allow_env: false,
        allow_home: false,
        strict: false,
        dry_run: false,
        warn_unused: false,
    }
}

#[test]
fn test_bin_run_single_pass() {
    let ws = TestWorkspace::new();
    let adoc = ws.root.join("test.adoc");
    // Ensure no spaces between comment and delimiter to avoid regex ambiguity
    // AND add the missing '=' suffix required for chunk definitions.
    std::fs::write(&adoc, "= Test\n\n[source,rust]\n----\n//<[@file test.rs]>=\nfn main() {}\n// @\n----\n").unwrap();

    let mut single = default_single_pass(&ws);
    single.directory = Some(ws.root.clone());

    println!("Running single pass on {:?}", ws.root);
    run_single_pass_from_cli(single, false).expect("single pass failed");

    let out = ws.gen_file("test.rs");
    println!("Checking output path: {:?}", out);
    if !out.exists() {
        if let Ok(entries) = std::fs::read_dir(&ws.root) {
            for entry in entries {
                println!("In root: {:?}", entry.unwrap().path());
            }
        }
        if let Ok(entries) = std::fs::read_dir(ws.gen_dir()) {
            for entry in entries {
                println!("In gen: {:?}", entry.unwrap().path());
            }
        }
    }
    assert!(out.exists(), "Output file test.rs should exist in gen dir");
    assert!(ws.db().exists(), "Database should exist");
}

#[test]
fn test_bin_run_multi_pass_error() {
    let ws = TestWorkspace::new();
    let config = ws.root.join("weaveback.toml");
    // Missing config should error
    let res = run_multi_pass(&config, false);
    assert!(res.is_err());
}

#[test]
fn test_bin_run_apply_back() {
    let ws = TestWorkspace::new();
    let mut db = ws.open_db();
    db.set_chunk_defs(&[weaveback_tangle::db::ChunkDefEntry {
        src_file: "test.adoc".to_string(),
        chunk_name: "@file test.rs".to_string(),
        nth: 0,
        def_start: 5,
        def_end: 7,
    }]).unwrap();
    db.set_baseline("test.rs", b"// <[@file test.rs]>\nfn old() {}\n// @\n").unwrap();

    let gen_file = ws.gen_file("test.rs");
    std::fs::create_dir_all(gen_file.parent().unwrap()).unwrap();
    std::fs::write(&gen_file, "// <[@file test.rs]>\nfn new() {}\n// @\n").unwrap();

    let single = default_single_pass(&ws);
    let res = run_apply_back(vec!["test.rs".to_string()], false, &single);
    let _ = res;
}

