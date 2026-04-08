use weaveback_macro::evaluator::{EvalConfig, EvalError};
use weaveback_tangle::WeavebackError;
use weaveback_core::PathResolver;
use clap::Parser;
use serde_json::json;
use std::path::{Path, PathBuf};

mod apply_back;
mod cli_generated;
mod lint;
mod lookup;
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
        discovery_mode: false,
        allow_env: args.allow_env,
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
fn open_db(db_path: &Path) -> Result<weaveback_tangle::db::WeavebackDb, Error> {
    weaveback_api::coverage::open_db(db_path).map_err(Error::from)
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

use weaveback_lsp::LspClient;

fn run_lsp(
    cmd: LspCommands,
    db_path: PathBuf,
    gen_dir: PathBuf,
    eval_config: EvalConfig,
    override_cmd: Option<String>,
    override_lang: Option<String>,
) -> Result<(), Error> {
    let project_root = std::env::current_dir()?;
    let db = open_db(&db_path)?;
    let resolver = PathResolver::new(project_root.clone(), gen_dir);

    // Determine LSP config based on input file or overrides.
    let sample_file = match &cmd {
        LspCommands::Definition { out_file, .. } => out_file,
        LspCommands::References { out_file, .. } => out_file,
    };
    let ext = Path::new(sample_file).extension().and_then(|e| e.to_str()).unwrap_or("");

    let (lsp_cmd, lsp_lang) = match (override_cmd, override_lang) {
        (Some(c), Some(l)) => (c, l),
        (c, l) => {
            let (def_cmd, def_lang) = weaveback_lsp::get_lsp_config(ext)
                .ok_or_else(|| Error::Io(std::io::Error::other(format!("unsupported file extension: .{}", ext))))?;
            (c.unwrap_or(def_cmd), l.unwrap_or(def_lang))
        }
    };

    let mut client = LspClient::spawn(&lsp_cmd, &[], &project_root, lsp_lang)
        .map_err(|e| Error::Io(std::io::Error::other(format!("failed to start LSP '{}': {e}", lsp_cmd))))?;

    client.initialize(&project_root)
        .map_err(|e| Error::Io(std::io::Error::other(format!("LSP initialization failed: {e}"))))?;

    match cmd {
        LspCommands::Definition { out_file, line, col } => {
            let path = Path::new(&out_file).canonicalize()
                .map_err(|e| Error::Io(std::io::Error::other(format!("invalid file path '{}': {e}", out_file))))?;

            client.did_open(&path)
                .map_err(|e| Error::Io(std::io::Error::other(format!("LSP didOpen failed: {e}"))))?;

            let loc = client.goto_definition(&path, line - 1, col - 1)
                .map_err(|e| Error::Io(std::io::Error::other(format!("LSP definition call failed: {e}"))))?;

            if let Some(loc) = loc {
                let target_path = loc.uri.to_file_path()
                    .map_err(|_| Error::Io(std::io::Error::other("LSP returned non-file URI")))?;
                let target_line = loc.range.start.line + 1;
                let target_col = loc.range.start.character + 1;

                // Map back to source
                let trace = lookup::perform_trace(
                    &target_path.to_string_lossy(),
                    target_line,
                    target_col,
                    &db,
                    &resolver,
                    eval_config,
                ).map_err(|e| Error::Io(std::io::Error::other(format!("Mapping failed: {e:?}"))))?;

                if let Some(res) = trace {
                    println!("{}", serde_json::to_string_pretty(&res).unwrap());
                } else {
                    println!("{}", json!({
                        "out_file": target_path.to_string_lossy(),
                        "out_line": target_line,
                        "out_col":  target_col,
                        "note": "LSP result could not be mapped to source"
                    }));
                }
            } else {
                println!("No definition found.");
            }
        }
        LspCommands::References { out_file, line, col } => {
            let path = Path::new(&out_file).canonicalize()
                .map_err(|e| Error::Io(std::io::Error::other(format!("invalid file path '{}': {e}", out_file))))?;

            client.did_open(&path)
                .map_err(|e| Error::Io(std::io::Error::other(format!("LSP didOpen failed: {e}"))))?;

            let locs = client.find_references(&path, line - 1, col - 1)
                .map_err(|e| Error::Io(std::io::Error::other(format!("LSP references call failed: {e}"))))?;

            let mut results = Vec::new();
            for loc in locs {
                let target_path = loc.uri.to_file_path()
                    .map_err(|_| Error::Io(std::io::Error::other("LSP returned non-file URI")))?;
                let target_line = loc.range.start.line + 1;
                let target_col = loc.range.start.character + 1;

                let trace = lookup::perform_trace(
                    &target_path.to_string_lossy(),
                    target_line,
                    target_col,
                    &db,
                    &resolver,
                    eval_config.clone(),
                ).map_err(|e| Error::Io(std::io::Error::other(format!("Mapping failed: {e:?}"))))?;

                if let Some(res) = trace {
                    results.push(res);
                } else {
                    results.push(json!({
                        "out_file": target_path.to_string_lossy(),
                        "out_line": target_line,
                        "out_col":  target_col,
                        "note": "LSP result could not be mapped to source"
                    }));
                }
            }
            println!("{}", serde_json::to_string_pretty(&results).unwrap());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use weaveback_api::coverage::*;
    use tempfile::tempdir;
    use weaveback_tangle::db::{Confidence, NowebMapEntry, WeavebackDb};

    #[test]
    fn parse_generated_location_accepts_line_and_optional_col() {
        assert_eq!(
            parse_generated_location("gen/out.rs:17").unwrap(),
            ("gen/out.rs".to_string(), 17, 1)
        );
        assert_eq!(
            parse_generated_location("gen/out.rs:17:9").unwrap(),
            ("gen/out.rs".to_string(), 17, 9)
        );
    }

    #[test]
    fn scan_generated_locations_extracts_unique_specs() {
        let text = "panic at src/generated.rs:1:27\nsee also gen/out.rs:17 and src/generated.rs:1:27";
        assert_eq!(
            scan_generated_locations(text),
            vec!["src/generated.rs:1:27", "gen/out.rs:17"]
        );
    }

    #[test]
    fn scan_generated_locations_trims_punctuation_and_supports_windows_paths() {
        let text = r#"note: (src/generated.rs:1:27), "C:\tmp\gen\out.rs:17:9"."#;
        assert_eq!(
            scan_generated_locations(text),
            vec!["src/generated.rs:1:27", r#"C:\tmp\gen\out.rs:17:9"#]
        );
    }

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

    #[test]
    fn emit_augmented_cargo_message_attaches_full_trace_json() {
        let line = r#"{"reason":"compiler-message","message":{"spans":[]}}"#;
        let records = vec![json!({
            "out_file": "gen/out.rs",
            "out_line": 17,
            "out_col": 9,
            "src_file": "src/doc.adoc",
            "src_line": 42,
            "src_col": 3,
            "chunk": "main",
            "kind": "Literal",
            "source_section_breadcrumb": ["Root", "Topic"],
            "source_section_prose": "Explain."
        })];
        let span_records = vec![json!({
            "generated_file": "gen/out.rs",
            "generated_line": 17,
            "generated_col": 9,
            "is_primary": true,
            "trace": records[0].clone(),
        })];
        let mut out = Vec::new();
        emit_augmented_cargo_message(line, records, span_records, &mut out).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let attrs = value["weaveback_attributions"].as_array().unwrap();
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0]["chunk"], "main");
        assert_eq!(attrs[0]["source_section_breadcrumb"], json!(["Root", "Topic"]));
        assert_eq!(attrs[0]["source_section_prose"], "Explain.");
        let span_attrs = value["weaveback_span_attributions"].as_array().unwrap();
        assert_eq!(span_attrs[0]["generated_file"], "gen/out.rs");
        assert_eq!(span_attrs[0]["trace"]["chunk"], "main");
        assert_eq!(value["weaveback_source_summary"]["sources"][0]["src_file"], "src/doc.adoc");
        assert_eq!(
            value["weaveback_source_summary"]["sources"][0]["sections"][0]["source_section_breadcrumb"],
            json!(["Root", "Topic"])
        );
        assert_eq!(
            value["weaveback_source_summary"]["sources"][0]["sections"][0]["generated_spans"][0]["generated_file"],
            "gen/out.rs"
        );
    }

    #[test]
    fn emit_text_attribution_message_wraps_plain_text_line() {
        let mut out = Vec::new();
        emit_text_attribution_message(
            "stderr",
            "panic at src/generated.rs:1:27",
            vec![json!({
                "location": "src/generated.rs:1:27",
                "ok": true,
                "trace": {"expanded_file": "src/doc.adoc", "chunk": "generated"},
            })],
            &mut out,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["reason"], "weaveback-text-attribution");
        assert_eq!(value["stream"], "stderr");
        assert_eq!(value["text"], "panic at src/generated.rs:1:27");
        assert_eq!(value["weaveback_attributions"][0]["location"], "src/generated.rs:1:27");
        assert_eq!(
            value["weaveback_source_summary"]["sources"][0]["src_file"],
            "src/doc.adoc"
        );
    }

    #[test]
    fn collect_text_attributions_scans_and_traces_locations() {
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
        let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("gen"));
        let records = collect_text_attributions(
            "panic at out.rs:1 and out.rs:1",
            Some(&db),
            Path::new("."),
            &resolver,
            &EvalConfig::default(),
        );
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["location"], "out.rs:1");
        assert_eq!(records[0]["ok"], true);
        assert_eq!(records[0]["trace"]["chunk"], "main");
    }

    #[test]
    fn build_location_attribution_summary_groups_successful_records() {
        let summary = build_location_attribution_summary(&[
            json!({
                "location": "out.rs:1",
                "ok": true,
                "trace": {
                    "src_file": "src/doc.adoc",
                    "chunk": "main",
                    "source_section_breadcrumb": ["Root", "Topic"],
                    "source_section_prose": "Explain."
                },
            }),
            json!({
                "location": "out.rs:2",
                "ok": false,
                "trace": serde_json::Value::Null,
            }),
        ]);
        assert_eq!(summary["count"], 1);
        assert_eq!(summary["sources"][0]["src_file"], "src/doc.adoc");
        assert_eq!(
            summary["sources"][0]["sections"][0]["locations"],
            json!(["out.rs:1"])
        );
    }

    #[test]
    fn emit_cargo_summary_message_emits_final_grouped_json() {
        let span_records = vec![
            json!({
                "generated_file": "gen/out.rs",
                "generated_line": 17,
                "generated_col": 9,
                "is_primary": true,
                "trace": {
                    "src_file": "src/doc.adoc",
                    "chunk": "main",
                    "source_section_breadcrumb": ["Root", "Topic"],
                    "source_section_prose": "Explain."
                },
            }),
            json!({
                "generated_file": "gen/out.rs",
                "generated_line": 20,
                "generated_col": 1,
                "is_primary": false,
                "trace": {
                    "src_file": "src/doc.adoc",
                    "chunk": "helper",
                    "source_section_breadcrumb": ["Root", "Topic"],
                    "source_section_prose": "Explain."
                },
            }),
        ];
        let mut out = Vec::new();
        emit_cargo_summary_message(3, &span_records, &mut out).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["reason"], "weaveback-summary");
        assert_eq!(value["compiler_message_count"], 3);
        assert_eq!(value["generated_span_count"], 2);
        assert_eq!(value["weaveback_source_summary"]["sources"][0]["src_file"], "src/doc.adoc");
        assert_eq!(
            value["weaveback_source_summary"]["sources"][0]["sections"][0]["chunks"],
            json!(["helper", "main"])
        );
    }

    #[test]
    fn run_cargo_annotated_to_writer_traces_real_generated_compile_error() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        std::fs::write(
            root.join("Cargo.toml"),
            r#"[package]
name = "wb-fixture"
version = "0.1.0"
edition = "2024"
"#,
        )
        .expect("Cargo.toml");
        std::fs::write(
            root.join("src/main.rs"),
            "mod generated;\nfn main() { generated::broken(); }\n",
        )
        .expect("main");
        std::fs::write(
            root.join("src/generated.rs"),
            "pub fn broken() { let x = ; }\n",
        )
        .expect("generated");

        let db_path = root.join("weaveback.db");
        let mut db = WeavebackDb::open(&db_path).expect("db");
        db.set_noweb_entries(
            "src/generated.rs",
            &[(
                0,
                NowebMapEntry {
                    src_file: "src/doc.adoc".to_string(),
                    chunk_name: "generated".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("noweb");
        db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Generated\nThe generated body.\n")
            .expect("snapshot");

        let mut out = Vec::new();
        let err = run_cargo_annotated_to_writer(
            vec!["check".to_string(), "--quiet".to_string()],
            true,
            db_path,
            root.join("gen"),
            EvalConfig::default(),
            root,
            &mut out,
        )
        .expect_err("cargo should fail on generated syntax error");
        let rendered = String::from_utf8(out).expect("utf8");
        let lines = rendered
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json line"))
            .collect::<Vec<_>>();

        assert!(matches!(err, CoverageError::Io(_)));
        let compiler = lines
            .iter()
            .find(|value| value["reason"] == "compiler-message")
            .expect("compiler message");
        let span_attrs = compiler["weaveback_span_attributions"]
            .as_array()
            .expect("span attributions");
        assert!(!span_attrs.is_empty());
        assert!(span_attrs.iter().any(|record| {
            record["trace"]["src_file"]
                .as_str()
                .or_else(|| record["trace"]["expanded_file"].as_str())
                .is_some_and(|path| path.ends_with("src/doc.adoc"))
                && record["trace"]["source_section_breadcrumb"] == json!(["Root", "Generated"])
        }));

        let summary = lines
            .iter()
            .find(|value| value["reason"] == "weaveback-summary")
            .expect("summary");
        let sections = summary["weaveback_source_summary"]["sources"][0]["sections"]
            .as_array()
            .expect("sections");
        assert!(sections.iter().any(|section| {
            section["source_section_breadcrumb"] == json!(["Root", "Generated"])
                && section["generated_spans"]
                    .as_array()
                    .is_some_and(|spans| spans.iter().any(|span| {
                        span["generated_file"]
                            .as_str()
                            .is_some_and(|file| file.ends_with("src/generated.rs"))
                    }))
        }));
    }

    #[test]
    fn run_cargo_annotated_to_writer_emits_text_attribution_for_text_warning() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        std::fs::write(
            root.join("Cargo.toml"),
            r#"[package]
name = "wb-fixture-warning"
version = "0.1.0"
edition = "2024"
build = "build.rs"
"#,
        )
        .expect("Cargo.toml");
        std::fs::write(
            root.join("build.rs"),
            "fn main() { println!(\"cargo:warning=src/generated.rs:1:27\"); }\n",
        )
        .expect("build");
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() {}\n",
        )
        .expect("main");
        std::fs::write(
            root.join("src/generated.rs"),
            "pub fn generated() {}\n",
        )
        .expect("generated");

        let db_path = root.join("weaveback.db");
        let mut db = WeavebackDb::open(&db_path).expect("db");
        db.set_noweb_entries(
            "src/generated.rs",
            &[(
                0,
                NowebMapEntry {
                    src_file: "src/doc.adoc".to_string(),
                    chunk_name: "generated".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("noweb");
        db.set_src_snapshot("src/doc.adoc", b"= Root\n\n== Generated\nThe generated body.\n")
            .expect("snapshot");

        let mut out = Vec::new();
        run_cargo_annotated_to_writer(
            vec![
                "check".to_string(),
            ],
            true,
            db_path,
            root.join("gen"),
            EvalConfig::default(),
            root,
            &mut out,
        )
        .expect("cargo check should succeed");
        let rendered = String::from_utf8(out).expect("utf8");
        let lines = rendered
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json line"))
            .collect::<Vec<_>>();

        let text_attr = lines
            .iter()
            .find(|value| value["reason"] == "weaveback-text-attribution")
            .expect("text attribution");
        assert_eq!(text_attr["stream"], "stderr");
        assert!(
            text_attr["weaveback_attributions"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| {
                    item["trace"]["source_section_breadcrumb"] == json!(["Root", "Generated"])
                }))
        );
    }

    #[test]
    fn parse_lcov_records_extracts_file_line_hits() {
        let text = "TN:\nSF:src/generated.rs\nDA:1,3\nDA:2,0\nend_of_record\nSF:other.rs\nDA:4,1\nend_of_record\n";
        assert_eq!(
            parse_lcov_records(text),
            vec![
                ("src/generated.rs".to_string(), 1, 3),
                ("src/generated.rs".to_string(), 2, 0),
                ("other.rs".to_string(), 4, 1),
            ]
        );
    }

    #[test]
    fn build_coverage_summary_groups_lines_by_source_section() {
        let mut db = WeavebackDb::open_temp().expect("db");
        db.set_noweb_entries(
            "src/generated.rs",
            &[
                (
                    0,
                    NowebMapEntry {
                        src_file: "src/doc.adoc".to_string(),
                        chunk_name: "generated".to_string(),
                        src_line: 3,
                        indent: String::new(),
                        confidence: Confidence::Exact,
                    },
                ),
                (
                    1,
                    NowebMapEntry {
                        src_file: "src/doc.adoc".to_string(),
                        chunk_name: "generated".to_string(),
                        src_line: 3,
                        indent: String::new(),
                        confidence: Confidence::Exact,
                    },
                ),
            ],
        )
        .expect("noweb");
        db.set_src_snapshot(
            "src/doc.adoc",
            b"= Root\n\n== Generated\nThe generated body.\n",
        )
        .expect("snapshot");
        let records = vec![
            ("src/generated.rs".to_string(), 1, 1),
            ("src/generated.rs".to_string(), 2, 0),
            ("unmapped.rs".to_string(), 9, 0),
        ];
        let project_root = PathBuf::from(".");
        let resolver = PathResolver::new(project_root.clone(), PathBuf::from("gen"));
        let summary = build_coverage_summary(
            &records,
            &db,
            &project_root,
            &resolver,
        );
        assert_eq!(summary["line_records"], 3);
        assert_eq!(summary["attributed_records"], 2);
        assert_eq!(summary["unattributed_records"], 1);
        assert!(
            summary["sources"][0]["src_file"]
                .as_str()
                .is_some_and(|path| path.ends_with("src/doc.adoc"))
        );
        assert_eq!(summary["sources"][0]["covered_lines"], 1);
        assert_eq!(summary["sources"][0]["missed_lines"], 1);
        assert_eq!(
            summary["sources"][0]["sections"][0]["source_section_breadcrumb"],
            json!(["Root", "Generated"])
        );
        assert_eq!(
            summary["sources"][0]["sections"][0]["generated_lines"][0]["generated_file"],
            "src/generated.rs"
        );
        assert_eq!(summary["unattributed"][0]["generated_file"], "unmapped.rs");
        assert_eq!(summary["unattributed_files"][0]["generated_file"], "unmapped.rs");
        assert_eq!(summary["unattributed_files"][0]["missed_lines"], 1);
        assert_eq!(summary["unattributed_files"][0]["has_noweb_entries"], false);
    }

    #[test]
    fn build_coverage_summary_marks_partial_unattributed_files() {
        let mut db = WeavebackDb::open_temp().expect("db");
        db.set_noweb_entries(
            "src/generated.rs",
            &[(
                0,
                NowebMapEntry {
                    src_file: "src/doc.adoc".to_string(),
                    chunk_name: "generated".to_string(),
                    src_line: 3,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("noweb");
        db.set_src_snapshot(
            "src/doc.adoc",
            b"= Root\n\n== Generated\nThe generated body.\n",
        )
        .expect("snapshot");
        let records = vec![
            ("src/generated.rs".to_string(), 2, 0),
        ];
        let project_root = PathBuf::from(".");
        let resolver = PathResolver::new(project_root.clone(), PathBuf::from("gen"));
        let summary = build_coverage_summary(&records, &db, &project_root, &resolver);
        assert_eq!(summary["unattributed_records"], 1);
        assert_eq!(summary["unattributed_files"][0]["generated_file"], "src/generated.rs");
        assert_eq!(summary["unattributed_files"][0]["has_noweb_entries"], true);
        assert_eq!(summary["unattributed_files"][0]["mapped_line_start"], 1);
        assert_eq!(summary["unattributed_files"][0]["mapped_line_end"], 1);
    }

    #[test]
    fn build_coverage_summary_sorts_sources_and_sections_by_missed_lines() {
        let mut db = WeavebackDb::open_temp().expect("db");
        db.set_noweb_entries(
            "src/a_generated.rs",
            &[
                (
                    0,
                    NowebMapEntry {
                        src_file: "src/a.adoc".to_string(),
                        chunk_name: "alpha".to_string(),
                        src_line: 3,
                        indent: String::new(),
                        confidence: Confidence::Exact,
                    },
                ),
                (
                    1,
                    NowebMapEntry {
                        src_file: "src/a.adoc".to_string(),
                        chunk_name: "alpha".to_string(),
                        src_line: 6,
                        indent: String::new(),
                        confidence: Confidence::Exact,
                    },
                ),
            ],
        )
        .expect("a noweb");
        db.set_noweb_entries(
            "src/b_generated.rs",
            &[
                (
                    0,
                    NowebMapEntry {
                        src_file: "src/b.adoc".to_string(),
                        chunk_name: "beta".to_string(),
                        src_line: 3,
                        indent: String::new(),
                        confidence: Confidence::Exact,
                    },
                ),
                (
                    1,
                    NowebMapEntry {
                        src_file: "src/b.adoc".to_string(),
                        chunk_name: "beta".to_string(),
                        src_line: 3,
                        indent: String::new(),
                        confidence: Confidence::Exact,
                    },
                ),
            ],
        )
        .expect("b noweb");
        db.set_src_snapshot("src/a.adoc", b"= Root\n\n== A1\none\n\n== A2\ntwo\n")
            .expect("a snapshot");
        db.set_src_snapshot("src/b.adoc", b"= Root\n\n== B\nbody\n")
            .expect("b snapshot");
        let records = vec![
            ("src/a_generated.rs".to_string(), 1, 1),
            ("src/a_generated.rs".to_string(), 2, 0),
            ("src/b_generated.rs".to_string(), 1, 0),
            ("src/b_generated.rs".to_string(), 2, 0),
        ];
        let project_root = PathBuf::from(".");
        let resolver = PathResolver::new(project_root.clone(), PathBuf::from("gen"));
        let summary = build_coverage_summary(
            &records,
            &db,
            &project_root,
            &resolver,
        );
        let sources = summary["sources"].as_array().expect("sources");
        assert!(
            sources[0]["src_file"]
                .as_str()
                .is_some_and(|path| path.ends_with("src/b.adoc"))
        );
        let a_sections = sources[1]["sections"].as_array().expect("sections");
        assert_eq!(a_sections[0]["source_section_breadcrumb"], json!(["Root", "A2"]));
        assert_eq!(a_sections[0]["missed_lines"], 1);
        assert_eq!(a_sections[1]["source_section_breadcrumb"], json!(["Root", "A1"]));
    }

    #[test]
    fn build_coverage_summary_view_keeps_ranked_top_slices() {
        let summary = json!({
            "line_records": 3,
            "attributed_records": 3,
            "unattributed_records": 0,
            "unattributed_files": [
                {
                    "generated_file": "gen/a.rs",
                    "missed_lines": 3
                },
                {
                    "generated_file": "gen/b.rs",
                    "missed_lines": 1
                }
            ],
            "sources": [
                {
                    "src_file": "src/a.adoc",
                    "sections": [
                        {"source_section_breadcrumb": ["Root", "A1"]},
                        {"source_section_breadcrumb": ["Root", "A2"]}
                    ]
                },
                {
                    "src_file": "src/b.adoc",
                    "sections": [
                        {"source_section_breadcrumb": ["Root", "B1"]}
                    ]
                }
            ]
        });
        let view = build_coverage_summary_view(&summary, 1, 1);
        assert_eq!(view["summary_view"]["top_sources"], 1);
        assert_eq!(view["summary_view"]["top_sections"], 1);
        assert_eq!(view["summary_view"]["sources"].as_array().unwrap().len(), 1);
        assert_eq!(view["summary_view"]["unattributed_files"].as_array().unwrap().len(), 1);
        assert_eq!(view["summary_view"]["unattributed_files"][0]["generated_file"], "gen/a.rs");
        assert_eq!(
            view["summary_view"]["sources"][0]["sections"].as_array().unwrap().len(),
            1
        );
    }

    #[test]
    fn collect_cargo_attributions_maps_generated_span_back_to_source() {
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
        let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("gen"));
        let diagnostic = CargoDiagnostic {
            spans: vec![CargoDiagnosticSpan {
                file_name: "out.rs".to_string(),
                line_start: 1,
                column_start: 1,
                is_primary: true,
            }],
        };

        let records = collect_cargo_attributions(
            &diagnostic,
            Some(&db),
            Path::new("."),
            &resolver,
            &EvalConfig::default(),
        );
        assert_eq!(records.len(), 1);
        assert!(
            records[0]["src_file"]
                .as_str()
                .is_some_and(|path| path.ends_with("src/doc.adoc"))
        );
        assert_eq!(records[0]["src_line"], 4);
        assert_eq!(records[0]["chunk"], "main");
        assert_eq!(records[0]["source_section_breadcrumb"], json!(["Root", "Topic"]));
    }

    #[test]
    fn collect_cargo_span_attributions_keeps_generated_span_context() {
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
        let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("gen"));
        let diagnostic = CargoDiagnostic {
            spans: vec![
                CargoDiagnosticSpan {
                    file_name: "out.rs".to_string(),
                    line_start: 1,
                    column_start: 1,
                    is_primary: true,
                },
                CargoDiagnosticSpan {
                    file_name: "out.rs".to_string(),
                    line_start: 1,
                    column_start: 5,
                    is_primary: false,
                },
            ],
        };

        let records = collect_cargo_span_attributions(
            &diagnostic,
            Some(&db),
            Path::new("."),
            &resolver,
            &EvalConfig::default(),
        );
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["generated_file"], "out.rs");
        assert_eq!(records[0]["trace"]["chunk"], "main");
        assert_eq!(records[1]["is_primary"], false);
    }

    #[test]
    fn build_cargo_attribution_summary_groups_by_source_file() {
        let summary = build_cargo_attribution_summary(&[
            json!({
                "generated_file": "out.rs",
                "generated_line": 1,
                "generated_col": 1,
                "is_primary": true,
                "trace": {
                    "src_file": "src/a.adoc",
                    "chunk": "alpha",
                    "source_section_breadcrumb": ["Root", "Alpha"],
                    "source_section_prose": "Alpha prose."
                }
            }),
            json!({
                "generated_file": "out.rs",
                "generated_line": 2,
                "generated_col": 1,
                "is_primary": false,
                "trace": {
                    "src_file": "src/a.adoc",
                    "chunk": "beta",
                    "source_section_breadcrumb": ["Root", "Alpha"],
                    "source_section_prose": "Alpha prose."
                }
            }),
            json!({
                "generated_file": "out2.rs",
                "generated_line": 1,
                "generated_col": 1,
                "is_primary": true,
                "trace": {
                    "src_file": "src/b.adoc",
                    "chunk": "gamma",
                    "source_section_breadcrumb": ["Root", "Beta"],
                    "source_section_prose": "Beta prose."
                }
            }),
        ]);
        assert_eq!(summary["count"], 3);
        assert_eq!(summary["sources"][0]["src_file"], "src/a.adoc");
        assert_eq!(summary["sources"][0]["count"], 2);
        assert_eq!(
            summary["sources"][0]["sections"][0]["source_section_breadcrumb"],
            json!(["Root", "Alpha"])
        );
        assert_eq!(
            summary["sources"][0]["sections"][0]["generated_spans"][0]["generated_file"],
            "out.rs"
        );
        assert_eq!(summary["sources"][1]["src_file"], "src/b.adoc");
    }
}
