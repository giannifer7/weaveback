// wb-query/src/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use std::path::PathBuf;

struct TestWorkspace {
    root: PathBuf,
}

impl TestWorkspace {
    fn new() -> Self {
        let unique = format!(
            "wb-query-tests-{}-{}",
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

    fn open_db(&mut self) -> weaveback_tangle::db::WeavebackDb {
        weaveback_tangle::db::WeavebackDb::open(self.db()).unwrap()
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn test_build_eval_config() {
    let cfg = super::build_eval_config('%', "a:b".to_string(), true);
    assert_eq!(cfg.sigil, '%');
    assert_eq!(cfg.include_paths.len(), 2);
    assert!(cfg.allow_env);
}

#[test]
fn run_where_missing_db() {
    let ws = TestWorkspace::new();
    let cli = Cli {
        db: ws.db(),
        gen_dir: ws.gen_dir(),
        command: Commands::Where {
            out_file: "test.rs".to_string(),
            line: 1,
        },
    };
    let res = run(cli);
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("weaveback.db"));
}

#[test]
fn run_where_success() {
    let mut ws = TestWorkspace::new();
    let mut db = ws.open_db();
    db.set_chunk_defs(&[weaveback_tangle::db::ChunkDefEntry {
        src_file: "test.adoc".to_string(),
        chunk_name: "test".to_string(),
        nth: 0,
        def_start: 1,
        def_end: 10,
    }]).unwrap();
    db.set_baseline("test.rs", b"content").unwrap();
    // Seed some attribution data if necessary, but perform_where mostly uses baseline/chunks.

    let cli = Cli {
        db: ws.db(),
        gen_dir: ws.gen_dir(),
        command: Commands::Where {
            out_file: "test.rs".to_string(),
            line: 1,
        },
    };
    // This won't find much without proper attribution, but it exercises the run() dispatch.
    let res = run(cli);
    assert!(res.is_ok());
}

#[test]
fn run_impact_success() {
    let mut ws = TestWorkspace::new();
    ws.open_db();
    let cli = Cli {
        db: ws.db(),
        gen_dir: ws.gen_dir(),
        command: Commands::Impact {
            chunk: "test".to_string(),
        },
    };
    let res = run(cli);
    assert!(res.is_ok());
}

#[test]
fn run_graph_success() {
    let mut ws = TestWorkspace::new();
    ws.open_db();
    let cli = Cli {
        db: ws.db(),
        gen_dir: ws.gen_dir(),
        command: Commands::Graph {
            chunk: None,
        },
    };
    let res = run(cli);
    assert!(res.is_ok());
}

#[test]
fn run_tags_success() {
    let mut ws = TestWorkspace::new();
    ws.open_db();
    let cli = Cli {
        db: ws.db(),
        gen_dir: ws.gen_dir(),
        command: Commands::Tags {
            file: None,
        },
    };
    let res = run(cli);
    assert!(res.is_ok());
}

#[test]
fn run_search_success() {
    let mut ws = TestWorkspace::new();
    let mut db = ws.open_db();
    db.rebuild_prose_fts(None).unwrap();
    let cli = Cli {
        db: ws.db(),
        gen_dir: ws.gen_dir(),
        command: Commands::Search {
            query: "test".to_string(),
            limit: 10,
        },
    };
    let res = run(cli);
    assert!(res.is_ok());
}

#[test]
fn run_tag_success() {
    let mut ws = TestWorkspace::new();
    ws.open_db();
    let config_path = ws.root.join("weaveback.toml");
    std::fs::write(&config_path, "[tags]\nbackend=\"ollama\"\n").unwrap();
    let cli = Cli {
        db: ws.db(),
        gen_dir: ws.gen_dir(),
        command: Commands::Tag {
            config: config_path,
            backend: None,
            model: None,
            endpoint: None,
            batch_size: None,
        },
    };
    let res = run(cli);
    assert!(res.is_ok());
}

#[test]
fn run_lint_success() {
    let ws = TestWorkspace::new();
    let adoc = ws.root.join("test.adoc");
    std::fs::write(&adoc, "= Test\n\n[source,rust]\n----\n// <<@file test.rs>>=\nfn main() {}\n// @\n----\n").unwrap();
    let cli = Cli {
        db: ws.db(),
        gen_dir: ws.gen_dir(),
        command: Commands::Lint {
            paths: vec![ws.root.clone()],
            strict: false,
            rule: None,
            json: false,
        },
    };
    let res = run(cli);
    assert!(res.is_ok());
}

#[test]
fn run_attribute_success() {
    let mut ws = TestWorkspace::new();
    ws.open_db();
    let cli = Cli {
        db: ws.db(),
        gen_dir: ws.gen_dir(),
        command: Commands::Attribute {
            scan_stdin: false,
            summary: false,
            locations: vec!["test.rs:1".to_string()],
            sigil: '%',
            include: ".".to_string(),
            allow_env: false,
        },
    };
    let res = run(cli);
    assert!(res.is_ok());
}

#[test]
fn run_coverage_success() {
    let mut ws = TestWorkspace::new();
    ws.open_db();
    let lcov = ws.root.join("lcov.info");
    std::fs::write(&lcov, "SF:test.rs\nDA:1,1\nend_of_record\n").unwrap();
    let cli = Cli {
        db: ws.db(),
        gen_dir: ws.gen_dir(),
        command: Commands::Coverage {
            summary: true,
            top_sources: 10,
            top_sections: 3,
            explain_unattributed: false,
            lcov_file: lcov,
        },
    };
    let res = run(cli);
    assert!(res.is_ok());
}

