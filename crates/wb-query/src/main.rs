mod cli_generated;
use cli_generated::{Cli, Commands};
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
            use weaveback_macro::evaluator::EvalConfig;
            let pathsep = default_pathsep();
            let include_paths: Vec<PathBuf> = include
                .split(&pathsep)
                .map(PathBuf::from)
                .collect();
            let eval_config = EvalConfig {
                sigil,
                include_paths,
                allow_env,
                ..Default::default()
            };
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
            use weaveback_macro::evaluator::EvalConfig;
            let pathsep = default_pathsep();
            let include_paths: Vec<PathBuf> = include
                .split(&pathsep)
                .map(PathBuf::from)
                .collect();
            let eval_config = EvalConfig {
                sigil,
                include_paths,
                allow_env,
                ..Default::default()
            };
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
            use weaveback_macro::evaluator::EvalConfig;
            let pathsep = default_pathsep();
            let include_paths: Vec<PathBuf> = include
                .split(&pathsep)
                .map(PathBuf::from)
                .collect();
            let eval_config = EvalConfig {
                sigil,
                include_paths,
                allow_env,
                ..Default::default()
            };
            weaveback_api::coverage::run_cargo_annotated(
                args, diagnostics_only, cli.db, cli.gen_dir, eval_config,
            )?;
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
