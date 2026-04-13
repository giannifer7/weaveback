mod cli_generated;
use cli_generated::{Cli, Commands, LspCommands};
use clap::Parser;
use std::path::PathBuf;
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
    Coverage(#[from] weaveback_api::coverage::CoverageError),
}

impl From<weaveback_tangle::db::DbError> for Error {
    fn from(e: weaveback_tangle::db::DbError) -> Self {
        Error::Noweb(WeavebackError::Db(e))
    }
}
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
            if let Err(e) = db.rebuild_prose_fts() {
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
