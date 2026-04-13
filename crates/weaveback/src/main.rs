use weaveback_macro::evaluator::{EvalConfig, EvalError};
use weaveback_tangle::WeavebackError;
use clap::Parser;
use std::path::PathBuf;

mod apply_back;
mod cli_generated;
mod lint;
mod mcp;
mod serve;
mod tag;

use cli_generated::{Args, Cli, Commands, LspCommands};

fn default_pathsep() -> String {
    if cfg!(windows) {
        ";".to_string()
    } else {
        ":".to_string()
    }
}

// CLI declarations live in cli_generated.rs, emitted from cli-spec.adoc.

use thiserror::Error;

#[derive(Debug, Error)]
enum Error {
    #[error("{0}")]
    Macro(#[from] EvalError),
    #[error("{0}")]
    Noweb(#[from] WeavebackError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<weaveback_tangle::db::DbError> for Error {
    fn from(e: weaveback_tangle::db::DbError) -> Self {
        Error::Noweb(WeavebackError::Db(e))
    }
}

impl From<weaveback_api::query::ApiError> for Error {
    fn from(e: weaveback_api::query::ApiError) -> Self {
        match e {
            weaveback_api::query::ApiError::Db(e) => Error::Noweb(WeavebackError::Db(e)),
            weaveback_api::query::ApiError::Io(e) => Error::Io(e),
        }
    }
}

impl From<weaveback_api::process::ProcessError> for Error {
    fn from(e: weaveback_api::process::ProcessError) -> Self {
        match e {
            weaveback_api::process::ProcessError::Tangle(e) => Error::Noweb(e),
            weaveback_api::process::ProcessError::Macro(e)  => Error::Macro(e),
            weaveback_api::process::ProcessError::Io(e)     => Error::Io(e),
        }
    }
}

use weaveback_api::process::SinglePassArgs;

fn run(args: Args) -> Result<(), Error> {
    if args.inputs.is_empty() && args.directory.is_none() {
        use clap::CommandFactory;
        Cli::command().print_help().unwrap();
        println!();
        std::process::exit(0);
    }

    weaveback_api::process::run_single_pass(SinglePassArgs {
        inputs:          args.inputs,
        directory:       args.directory,
        input_dir:       args.input_dir,
        gen_dir:         args.gen_dir,
        open_delim:      args.open_delim,
        close_delim:     args.close_delim,
        chunk_end:       args.chunk_end,
        comment_markers: args.comment_markers,
        ext:             args.ext,
        no_macros:       args.no_macros,
        dry_run:         args.dry_run,
        db:              args.db,
        depfile:         args.depfile,
        stamp:           args.stamp,
        strict:          args.strict,
        warn_unused:     args.warn_unused,
        allow_env:       args.allow_env,
        allow_home:      args.allow_home,
        force_generated: args.force_generated,
        sigil:           args.sigil,
        include:         args.include,
        formatter:       args.formatter,
        no_fts:          args.no_fts,
        dump_expanded:   args.dump_expanded,
    })?;
    Ok(())
}

fn build_eval_config(args: &Args) -> weaveback_macro::evaluator::EvalConfig {
    let pathsep = default_pathsep();
    let include_paths: Vec<std::path::PathBuf> = args.include.split(&pathsep).map(std::path::PathBuf::from).collect();
    weaveback_macro::evaluator::EvalConfig {
        sigil: args.sigil,
        include_paths,
        allow_env: args.allow_env,
        ..weaveback_macro::evaluator::EvalConfig::default()
    }
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Trace { out_file, line, col }) => {
            let eval_config = build_eval_config(&cli.args);
            run_trace(out_file, line, col, cli.args.db, cli.args.gen_dir, eval_config)
        }
        Some(Commands::Attribute {
            scan_stdin,
            summary,
            locations,
        }) => {
            let eval_config = build_eval_config(&cli.args);
            run_attribute(
                scan_stdin,
                summary,
                locations,
                cli.args.db,
                cli.args.gen_dir,
                eval_config,
            )
        }
        Some(Commands::Cargo {
            diagnostics_only,
            args,
        }) => {
            let eval_config = build_eval_config(&cli.args);
            run_cargo_annotated(
                args,
                diagnostics_only,
                cli.args.db,
                cli.args.gen_dir,
                eval_config,
            )
        }
        Some(Commands::Where { out_file, line }) => {
            run_where(out_file, line, cli.args.db, cli.args.gen_dir)
        }
        Some(Commands::Tangle { config, force_generated }) => {
            run_tangle_all(&config, force_generated)
        }
                Some(Commands::Tag { config, backend, model, endpoint, batch_size }) => {
            run_tag_only(&config, backend, model, endpoint, batch_size, cli.args.db)
        }
        Some(Commands::Mcp) => {
            let eval_config = build_eval_config(&cli.args);
            mcp::run_mcp(cli.args.db, cli.args.gen_dir, eval_config).map_err(Error::Io)
        }
        Some(Commands::ApplyBack { files, dry_run }) => {
            let eval_config = build_eval_config(&cli.args);
            let opts = apply_back::ApplyBackOptions {
                db_path: cli.args.db,
                gen_dir: cli.args.gen_dir,
                dry_run,
                files,
                eval_config: Some(eval_config),
            };
            apply_back::run_apply_back(opts, &mut std::io::stdout()).map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
        }
        Some(Commands::Impact { chunk }) => {
            run_impact(chunk, cli.args.db)
        }
        Some(Commands::Graph { chunk }) => {
            run_graph(chunk, cli.args.db)
        }
        Some(Commands::Search { query, limit }) => {
            run_search(query, limit, cli.args.db)
        }
        Some(Commands::Lint { paths, strict, rule, json }) => {
            lint::run_lint(paths, strict, rule, json)
                .map_err(|e| Error::Io(std::io::Error::other(e)))
        }
        Some(Commands::Coverage {
            summary,
            top_sources,
            top_sections,
            explain_unattributed,
            lcov_file,
        }) => {
            run_coverage(
                summary,
                top_sources,
                top_sections,
                explain_unattributed,
                lcov_file,
                cli.args.db,
                cli.args.gen_dir,
            )
        }
        Some(Commands::Tags { file }) => {
            run_tags(file, cli.args.db)
        }
        Some(Commands::Lsp { lsp_cmd, lsp_lang, cmd }) => {
            let eval_config = build_eval_config(&cli.args);
            run_lsp(cmd, cli.args.db, cli.args.gen_dir, eval_config, lsp_cmd, lsp_lang)
        }
                Some(Commands::Serve { port, html, open_delim, close_delim, chunk_end, comment_markers, ai_backend, ai_model, ai_endpoint, watch }) => {
            let backend = match ai_backend.as_str() {
                "anthropic" => serve::AiBackend::Anthropic,
                "gemini"    => serve::AiBackend::Gemini,
                "ollama"    => serve::AiBackend::Ollama,
                "openai"    => serve::AiBackend::OpenAi,
                _           => serve::AiBackend::ClaudeCli,
            };
            let tangle_cfg = serve::TangleConfig {
                open_delim,
                close_delim,
                chunk_end,
                comment_markers: comment_markers.split(',').map(|s| s.trim().to_string()).collect(),
                ai_backend: backend,
                ai_model,
                ai_endpoint,
            };
            serve::run_serve(port, html, tangle_cfg, watch)
                .map_err(|e| Error::Io(std::io::Error::other(e)))
        }
        None => run(cli.args),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

use weaveback_api::coverage::CoverageError;

impl From<CoverageError> for Error {
    fn from(e: CoverageError) -> Self {
        match e {
            CoverageError::Io(e)    => Error::Io(e),
            CoverageError::Noweb(e) => Error::Noweb(e),
        }
    }
}

fn run_where(out_file: String, line: u32, db_path: PathBuf, gen_dir: PathBuf) -> Result<(), Error> {
    weaveback_api::coverage::run_where(out_file, line, db_path, gen_dir).map_err(Error::from)
}
fn run_attribute(scan_stdin: bool, summary: bool, locations: Vec<String>, db_path: PathBuf, gen_dir: PathBuf, eval_config: weaveback_macro::evaluator::EvalConfig) -> Result<(), Error> {
    weaveback_api::coverage::run_attribute(scan_stdin, summary, locations, db_path, gen_dir, eval_config).map_err(Error::from)
}
fn run_coverage(summary_only: bool, top_sources: usize, top_sections: usize, explain_unattributed: bool, lcov_file: PathBuf, db_path: PathBuf, gen_dir: PathBuf) -> Result<(), Error> {
    weaveback_api::coverage::run_coverage(summary_only, top_sources, top_sections, explain_unattributed, lcov_file, db_path, gen_dir).map_err(Error::from)
}
fn run_cargo_annotated(cargo_args: Vec<String>, diagnostics_only: bool, db_path: PathBuf, gen_dir: PathBuf, eval_config: EvalConfig) -> Result<(), Error> {
    weaveback_api::coverage::run_cargo_annotated(cargo_args, diagnostics_only, db_path, gen_dir, eval_config).map_err(Error::from)
}
fn run_impact(chunk: String, db_path: PathBuf) -> Result<(), Error> {
    weaveback_api::coverage::run_impact(chunk, db_path).map_err(Error::from)
}
fn run_graph(chunk: Option<String>, db_path: PathBuf) -> Result<(), Error> {
    weaveback_api::coverage::run_graph(chunk, db_path).map_err(Error::from)
}
fn run_search(query: String, limit: usize, db_path: PathBuf) -> Result<(), Error> {
    weaveback_api::coverage::run_search(query, limit, db_path).map_err(Error::from)
}
fn run_tags(file: Option<String>, db_path: PathBuf) -> Result<(), Error> {
    weaveback_api::coverage::run_tags(file, db_path).map_err(Error::from)
}
fn run_trace(out_file: String, line: u32, col: u32, db_path: PathBuf, gen_dir: PathBuf, eval_config: weaveback_macro::evaluator::EvalConfig) -> Result<(), Error> {
    weaveback_api::coverage::run_trace(out_file, line, col, db_path, gen_dir, eval_config).map_err(Error::from)
}

use weaveback_api::tangle::{TangleCfg, TagsCfg};
use weaveback_api::tangle::{default_tags_backend, default_tags_model, default_tags_batch_size};

fn run_tangle_all(config_path: &std::path::Path, force_generated: bool) -> Result<(), Error> {
    weaveback_api::tangle::run_tangle_all(config_path, force_generated).map_err(Error::Io)
}

fn run_tag_only(
    config_path: &std::path::Path,
    backend_override:    Option<String>,
    model_override:      Option<String>,
    endpoint_override:   Option<String>,
    batch_size_override: Option<usize>,
    db_path: PathBuf,
) -> Result<(), Error> {
    let toml_tags: Option<TagsCfg> = std::fs::read_to_string(config_path).ok()
        .and_then(|s| toml::from_str::<TangleCfg>(&s).ok())
        .and_then(|c| c.tags);

    let tag_cfg = tag::TagConfig {
        backend:    backend_override
            .or_else(|| toml_tags.as_ref().map(|t| t.backend.clone()))
            .unwrap_or_else(default_tags_backend),
        model:      model_override
            .or_else(|| toml_tags.as_ref().map(|t| t.model.clone()))
            .unwrap_or_else(default_tags_model),
        endpoint:   endpoint_override
            .or_else(|| toml_tags.as_ref().and_then(|t| t.endpoint.clone())),
        batch_size: batch_size_override
            .or_else(|| toml_tags.as_ref().map(|t| t.batch_size))
            .unwrap_or_else(default_tags_batch_size),
    };

    if !db_path.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Database not found at {}. Run weaveback on your source files first.", db_path.display()),
        )));
    }

    match weaveback_tangle::db::WeavebackDb::open(&db_path) {
        Ok(mut db) => {
            tag::run_auto_tag(&mut db, &tag_cfg);
            if let Err(e) = db.rebuild_prose_fts() {
                eprintln!("warning: FTS index rebuild failed: {e}");
            }
        }
        Err(e) => return Err(Error::Io(std::io::Error::other(e.to_string()))),
    }
    Ok(())
}

fn run_lsp(
    cmd: LspCommands,
    db_path: PathBuf,
    gen_dir: PathBuf,
    eval_config: EvalConfig,
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

#[cfg(test)]
mod tests {
    use super::*;
    use weaveback_api::coverage::*;
    use weaveback_api::lookup;
    use weaveback_core::PathResolver;
    use weaveback_macro::evaluator::EvalConfig;
    use weaveback_tangle::db::{Confidence, NowebMapEntry, WeavebackDb};
    use cli_generated::{Cli, Commands};
    use clap::Parser;
    use serde_json::json;
    #[test]
    fn attribute_command_options_support_multiple_locations() {
        let cli = Cli::try_parse_from([
            "weaveback",
            "attribute",
            "gen/out.rs:17",
            "gen/out.rs:18:3",
        ])
        .unwrap();
        match cli.command.unwrap() {
            Commands::Attribute {
                scan_stdin,
                summary,
                locations,
            } => {
                assert!(!scan_stdin);
                assert!(!summary);
                assert_eq!(locations, vec!["gen/out.rs:17", "gen/out.rs:18:3"]);
            }
            _ => panic!("expected attribute command"),
        }
    }

    #[test]
    fn attribute_command_options_support_scan_stdin() {
        let cli = Cli::try_parse_from([
            "weaveback",
            "attribute",
            "--scan-stdin",
        ])
        .unwrap();
        match cli.command.unwrap() {
            Commands::Attribute {
                scan_stdin,
                summary,
                locations,
            } => {
                assert!(scan_stdin);
                assert!(!summary);
                assert!(locations.is_empty());
            }
            _ => panic!("expected attribute command"),
        }
    }

    #[test]
    fn attribute_command_options_support_summary() {
        let cli = Cli::try_parse_from([
            "weaveback",
            "attribute",
            "--summary",
            "--scan-stdin",
        ])
        .unwrap();
        match cli.command.unwrap() {
            Commands::Attribute {
                scan_stdin,
                summary,
                locations,
            } => {
                assert!(scan_stdin);
                assert!(summary);
                assert!(locations.is_empty());
            }
            _ => panic!("expected attribute command"),
        }
    }

    #[test]
    fn coverage_command_options_support_summary() {
        let cli = Cli::try_parse_from([
            "weaveback",
            "coverage",
            "--summary",
            "lcov.info",
        ])
        .unwrap();
        match cli.command.unwrap() {
            Commands::Coverage {
                summary,
                top_sources,
                top_sections,
                explain_unattributed,
                lcov_file,
            } => {
                assert!(summary);
                assert!(!explain_unattributed);
                assert_eq!(top_sources, 10);
                assert_eq!(top_sections, 3);
                assert_eq!(lcov_file, PathBuf::from("lcov.info"));
            }
            _ => panic!("expected coverage command"),
        }
    }

    #[test]
    fn scan_stdin_mode_keeps_bulk_json_shape_for_single_location() {
        let mut db = WeavebackDb::open_temp().expect("db");
        db.set_noweb_entries(
            "out.rs",
            &[(
                0,
                NowebMapEntry {
                    src_file: "src/doc.adoc".to_string(),
                    chunk_name: "main".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("noweb");
        db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Topic\nalpha\n")
            .expect("snapshot");
        let project_root = PathBuf::from(".");
        let resolver = PathResolver::new(project_root, PathBuf::from("gen"));
        let locations = vec!["out.rs:1".to_string()];
        let mut results = Vec::new();

        for location in locations {
            let (out_file, line, col) = parse_generated_location(&location).unwrap();
            match lookup::perform_trace(&out_file, line, col, &db, &resolver, EvalConfig::default()) {
                Ok(Some(json)) => results.push(json!({
                    "location": location,
                    "ok": true,
                    "trace": json,
                })),
                _ => panic!("expected attributed trace"),
            }
        }
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["location"], "out.rs:1");
        assert_eq!(results[0]["ok"], true);
        assert_eq!(results[0]["trace"]["chunk"], "main");
    }

    #[test]
    fn cargo_command_options_support_diagnostics_only() {
        let cli = Cli::try_parse_from([
            "weaveback",
            "cargo",
            "--diagnostics-only",
            "check",
            "-p",
            "weaveback",
        ])
        .unwrap();
        match cli.command.unwrap() {
            Commands::Cargo {
                diagnostics_only,
                args,
            } => {
                assert!(diagnostics_only);
                assert_eq!(args, vec!["check", "-p", "weaveback"]);
            }
            _ => panic!("expected cargo command"),
        }
    }
}
