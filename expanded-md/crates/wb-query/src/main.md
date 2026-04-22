# wb-query

`wb-query` is the weaveback analysis and metadata tool.  Most subcommands are
read-only queries over the database; `tag` is the one maintenance operation,
updating prose tags in place and rebuilding FTS afterward.

## CLI

Generated from `cli-spec/wb-query-cli.adoc`.


```rust
// <[wb-query-cli]>=
mod cli_generated;
use cli_generated::{Cli, Commands, LspCommands};
use clap::Parser;
use std::path::PathBuf;
// @
```


## Error Type


```rust
// <[wb-query-error]>=
use thiserror::Error;
use weaveback_tangle::WeavebackError;

#[derive(Debug, Error)]
enum Error {
    #[error("{0}")]
    Noweb(#[from] WeavebackError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Api(#[from] weaveback_api::query::ApiError),
    #[error("{0}")]
    Lookup(#[from] weaveback_api::lookup::LookupError),
    #[error("{0}")]
    Lint(String),
    #[error("{0}")]
    Coverage(#[from] weaveback_api::coverage::CoverageApiError),
}

impl From<weaveback_tangle::db::DbError> for Error {
    fn from(e: weaveback_tangle::db::DbError) -> Self {
        Error::Noweb(WeavebackError::Db(e))
    }
}
// @
```


## Dispatch


```rust
// <[wb-query-dispatch]>=
fn default_pathsep() -> String {
    if cfg!(windows) { ";".to_string() } else { ":".to_string() }
}

fn build_eval_config(sigil: char, include: String, allow_env: bool) -> weaveback_macro::evaluator::EvalConfig {
    use weaveback_macro::evaluator::EvalConfig;
    let pathsep = default_pathsep();
    let include_paths: Vec<PathBuf> = include
        .split(&pathsep)
        .map(PathBuf::from)
        .collect();
    EvalConfig {
        sigil,
        include_paths,
        allow_env,
        ..Default::default()
    }
}

fn run_tag_only(
    config_path: &std::path::Path,
    backend_override: Option<String>,
    model_override: Option<String>,
    endpoint_override: Option<String>,
    batch_size_override: Option<usize>,
    db_path: PathBuf,
) -> Result<(), Error> {
    use weaveback_api::tag;
    use weaveback_api::tangle::{TangleCfg, TagsCfg};
    use weaveback_api::tangle::{default_tags_backend, default_tags_batch_size, default_tags_model};

    let toml_tags: Option<TagsCfg> = std::fs::read_to_string(config_path).ok()
        .and_then(|s| toml::from_str::<TangleCfg>(&s).ok())
        .and_then(|c| c.tags);

    let tag_cfg = tag::TagConfig {
        backend: backend_override
            .or_else(|| toml_tags.as_ref().map(|t| t.backend.clone()))
            .unwrap_or_else(default_tags_backend),
        model: model_override
            .or_else(|| toml_tags.as_ref().map(|t| t.model.clone()))
            .unwrap_or_else(default_tags_model),
        endpoint: endpoint_override
            .or_else(|| toml_tags.as_ref().and_then(|t| t.endpoint.clone())),
        batch_size: batch_size_override
            .or_else(|| toml_tags.as_ref().map(|t| t.batch_size))
            .unwrap_or_else(default_tags_batch_size),
    };

    if !db_path.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Database not found at {}. Run wb-tangle first.", db_path.display()),
        )));
    }

    match weaveback_tangle::db::WeavebackDb::open(&db_path) {
        Ok(mut db) => {
            tag::run_auto_tag(&mut db, &tag_cfg);
            if let Err(e) = db.rebuild_prose_fts(None) {
                eprintln!("warning: FTS index rebuild failed: {e}");
            }
        }
        Err(e) => return Err(Error::Noweb(WeavebackError::Db(e))),
    }
    Ok(())
}

fn run_lsp(
    cmd: LspCommands,
    db_path: PathBuf,
    gen_dir: PathBuf,
    eval_config: weaveback_macro::evaluator::EvalConfig,
    override_cmd: Option<String>,
    override_lang: Option<String>,
) -> Result<(), Error> {
    let api_cmd = match cmd {
        LspCommands::Definition { out_file, line, col } =>
            weaveback_api::lsp_runner::LspCmd::Definition { out_file, line, col },
        LspCommands::References { out_file, line, col } =>
            weaveback_api::lsp_runner::LspCmd::References { out_file, line, col },
    };
    weaveback_api::lsp_runner::run_lsp(api_cmd, db_path, gen_dir, eval_config, override_cmd, override_lang)
        .map_err(Error::Io)
}

fn run(cli: Cli) -> Result<(), Error> {
    use weaveback_core::PathResolver;
    use weaveback_tangle::db::WeavebackDb;

    match cli.command {
        Commands::Where { out_file, line } => {
            let db = WeavebackDb::open_read_only(&cli.db)?;
            let resolver = PathResolver::new(PathBuf::from("."), cli.gen_dir);
            match weaveback_api::lookup::perform_where(&out_file, line, &db, &resolver)? {
                Some(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
                None    => println!("null"),
            }
        }

        Commands::Trace { out_file, line, col, sigil, include, allow_env } => {
            let eval_config = build_eval_config(sigil, include, allow_env);
            let db = WeavebackDb::open_read_only(&cli.db)?;
            let resolver = PathResolver::new(PathBuf::from("."), cli.gen_dir);
            match weaveback_api::lookup::perform_trace(&out_file, line, col, &db, &resolver, eval_config)? {
                Some(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
                None    => println!("null"),
            }
        }

        Commands::Impact { chunk } => {
            let v = weaveback_api::query::impact_analysis(&chunk, &cli.db)?;
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }

        Commands::Graph { chunk } => {
            let dot = weaveback_api::query::chunk_graph_dot(chunk.as_deref(), &cli.db)?;
            print!("{dot}");
        }

        Commands::Tag { config, backend, model, endpoint, batch_size } => {
            run_tag_only(&config, backend, model, endpoint, batch_size, cli.db)?;
        }

        Commands::Tags { file } => {
            let blocks = weaveback_api::query::list_block_tags(file.as_deref(), &cli.db)?;
            if blocks.is_empty() {
                eprintln!("No tagged blocks found. Add a [tags] section to weaveback.toml and run wb-tangle.");
            } else {
                let block_values: Vec<serde_json::Value> = blocks.iter().map(|b| serde_json::json!({
                    "src_file": b.src_file,
                    "block_index": b.block_index,
                    "block_type": b.block_type,
                    "line_start": b.line_start,
                    "tags": b.tags,
                })).collect();
                let v = serde_json::json!({ "tagged_blocks": block_values });
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
            }
        }

        Commands::Lint { paths, strict, rule, json } => {
            weaveback_api::lint::run_lint(paths, strict, rule, json)
                .map_err(Error::Lint)?;
        }

        Commands::Attribute { scan_stdin, summary, locations, sigil, include, allow_env } => {
            let eval_config = build_eval_config(sigil, include, allow_env);
            weaveback_api::coverage::run_attribute(
                scan_stdin, summary, locations, cli.db, cli.gen_dir, eval_config,
            )?;
        }

        Commands::Coverage { summary, top_sources, top_sections, explain_unattributed, lcov_file } => {
            weaveback_api::coverage::run_coverage(
                summary, top_sources, top_sections, explain_unattributed, lcov_file,
                cli.db, cli.gen_dir,
            )?;
        }

        Commands::Cargo { diagnostics_only, args, sigil, include, allow_env } => {
            let eval_config = build_eval_config(sigil, include, allow_env);
            weaveback_api::coverage::run_cargo_annotated(
                args, diagnostics_only, cli.db, cli.gen_dir, eval_config,
            )?;
        }

        Commands::Lsp { lsp_cmd, lsp_lang, sigil, include, allow_env, cmd } => {
            let eval_config = build_eval_config(sigil, include, allow_env);
            run_lsp(cmd, cli.db, cli.gen_dir, eval_config, lsp_cmd, lsp_lang)?;
        }

        Commands::Search { query, limit } => {
            weaveback_api::coverage::run_search(query, limit, cli.db)?;
        }
    }
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("wb-query: {e}");
        std::process::exit(1);
    }
}
// @
```


## Tests


```rust
// <[@file wb-query/src/tests.rs]>=
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

// @
```


## Assembly


```rust
// <[@file wb-query/src/main.rs]>=
// wb-query/src/main.rs
// I'd Really Rather You Didn't edit this generated file.

// <[wb-query-cli]>
// <[wb-query-error]>
// <[wb-query-dispatch]>
#[cfg(test)]
mod tests;

// @
```

