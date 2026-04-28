# Apply-Back Workspace Tests

Temporary workspace fixtures and source-map edge cases around the public runner.

```rust
// <[applyback-tests-workspace]>=
// ── run_apply_back empty database & diff edge cases ─────────────────────

struct TestWorkspace {
    root: std::path::PathBuf,
}
impl TestWorkspace {
    fn new() -> Self {
        let unique = format!(
            "wb-apply-back-tests-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }
    fn write_file(&self, rel: &str, content: &[u8]) {
        let path = self.root.join(rel);
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }
    fn open_db(&self) -> WeavebackDb {
        WeavebackDb::open(self.root.join("weaveback.db")).unwrap()
    }
}
impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn run_apply_back_early_exit_on_missing_db() {
    let ws = TestWorkspace::new();
    let opts = ApplyBackOptions {
        db_path: ws.root.join("missing.db"),
        gen_dir: ws.root.join("gen"),
        files: vec![],
        dry_run: false,
        eval_config: None,
    };
    let mut out = Vec::new();
    run_apply_back(opts, &mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("Database not found"));
}

#[test]
fn run_apply_back_no_modified_files() {
    let ws = TestWorkspace::new();
    let db = ws.open_db();
    db.set_baseline("out.rs", b"content").unwrap();
    ws.write_file("gen/out.rs", b"content");

    let opts = ApplyBackOptions {
        db_path: ws.root.join("weaveback.db"),
        gen_dir: ws.root.join("gen"),
        files: vec![],
        dry_run: false,
        eval_config: None,
    };
    let mut out = Vec::new();
    run_apply_back(opts, &mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("No modified gen/ files found"));
}

#[test]
fn run_apply_back_skips_missing_gen_files() {
    let ws = TestWorkspace::new();
    let db = ws.open_db();
    db.set_baseline("out.rs", b"content").unwrap();

    let opts = ApplyBackOptions {
        db_path: ws.root.join("weaveback.db"),
        gen_dir: ws.root.join("gen"),
        files: vec![],
        dry_run: false,
        eval_config: None,
    };
    let mut out = Vec::new();
    run_apply_back(opts, &mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("skip out.rs: file not found in gen/"));
}

#[test]
fn run_apply_back_reports_missing_source_map_on_diff() {
    let ws = TestWorkspace::new();
    let db = ws.open_db();
    db.set_baseline("out.rs", b"line1\nline2").unwrap();
    ws.write_file("gen/out.rs", b"line1\nmodified");

    let opts = ApplyBackOptions {
        db_path: ws.root.join("weaveback.db"),
        gen_dir: ws.root.join("gen"),
        files: vec![],
        dry_run: false,
        eval_config: None,
    };
    let mut out = Vec::new();
    run_apply_back(opts, &mut out).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("Processing out.rs"));
    assert!(s.contains("skip line 2: no source map entry"));
}
// @
```

