use thiserror::Error;
use weaveback_macro::evaluator::EvalError;
use weaveback_tangle::WeavebackError;

#[derive(Debug, Error)]
enum Error {
    #[error("{0}")]
    Macro(#[from] EvalError),
    #[error("{0}")]
    Noweb(#[from] WeavebackError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Process(#[from] weaveback_api::process::ProcessError),
}
use clap::{Parser, Args};
use std::path::PathBuf;

/// Weaveback tangle: literate programming build tool.
///
/// With no --dir flag: reads weaveback.toml and runs all [[pass]] entries.
/// With --dir DIR: single-pass mode — processes one directory of .adoc files.
#[derive(Parser, Debug)]
#[command(name = "wb-tangle", version)]
struct Cli {
    /// weaveback.toml config path (multi-pass mode only).
    #[arg(long, default_value = "weaveback.toml")]
    config: PathBuf,

    /// Overwrite generated files even if they differ from the stored baseline.
    #[arg(long)]
    force_generated: bool,

    #[command(flatten)]
    single: SinglePassCli,
}

/// All single-pass flags (used when --dir is present).
#[derive(Args, Debug)]
struct SinglePassCli {
    /// Directory to scan for driver files (activates single-pass mode).
    #[arg(long = "dir")]
    directory: Option<PathBuf>,

    /// Base directory for generated output files.
    #[arg(long = "gen", default_value = "gen")]
    gen_dir: PathBuf,

    /// Input files (single-pass explicit inputs, mutually exclusive with --dir).
    #[arg(required = false, conflicts_with = "directory")]
    inputs: Vec<PathBuf>,

    /// Base directory prepended to every input path.
    #[arg(long, default_value = ".")]
    input_dir: PathBuf,

    /// Chunk open delimiter.
    #[arg(long, default_value = "<[")]
    open_delim: String,

    /// Chunk close delimiter.
    #[arg(long, default_value = "]>")]
    close_delim: String,

    /// Chunk end marker.
    #[arg(long, default_value = "@")]
    chunk_end: String,

    /// Comment markers recognised before chunk delimiters (comma-separated).
    #[arg(long, default_value = "#,//")]
    comment_markers: String,

    /// Formatter command per output file extension, e.g. --formatter rs=rustfmt.
    #[arg(long, value_name = "EXT=CMD")]
    formatter: Vec<String>,

    /// File extension(s) to scan in --dir mode (can be repeated).
    #[arg(long, default_value = "md")]
    ext: Vec<String>,

    /// Path to the weaveback database.
    #[arg(long, default_value = "weaveback.db")]
    db: PathBuf,

    /// Write a Makefile depfile listing every source file read.
    #[arg(long)]
    depfile: Option<PathBuf>,

    /// Touch this file on success (build-system stamp).
    #[arg(long)]
    stamp: Option<PathBuf>,

    /// Skip rebuilding the prose FTS index after this run.
    #[arg(long, hide = true)]
    no_fts: bool,

    /// Skip macro expansion and feed source files directly to tangle.
    #[arg(long)]
    no_macros: bool,

    /// Allow %env(NAME) to read environment variables.
    #[arg(long)]
    allow_env: bool,

    /// Allow @file ~/... chunks to write outside the gen/ directory.
    #[arg(long)]
    allow_home: bool,

    /// Treat references to undefined chunks as fatal errors.
    #[arg(long)]
    strict: bool,

    /// Print output paths without writing anything.
    #[arg(long)]
    dry_run: bool,

    /// Warn about chunks that are defined but never referenced by any @file chunk.
    #[arg(long)]
    warn_unused: bool,

    /// Macro sigil character.
    #[arg(long, default_value = "%")]
    sigil: char,

    /// Include paths for %include/%import (colon-separated on Unix).
    #[arg(long, default_value = ".")]
    include: String,

    /// Print macro-expanded text to stderr before tangle processing.
    #[arg(long)]
    dump_expanded: bool,
}
fn run_multi_pass(config: &std::path::Path, force_generated: bool) -> Result<(), Error> {
    weaveback_api::tangle::run_tangle_all(config, force_generated).map_err(Error::Io)
}

fn run_single_pass_from_cli(s: SinglePassCli, force_generated: bool) -> Result<(), Error> {
    use weaveback_api::process::{SinglePassArgs, run_single_pass};
    run_single_pass(SinglePassArgs {
        inputs:          s.inputs,
        directory:       s.directory,
        input_dir:       s.input_dir,
        gen_dir:         s.gen_dir,
        open_delim:      s.open_delim,
        close_delim:     s.close_delim,
        chunk_end:       s.chunk_end,
        comment_markers: s.comment_markers,
        ext:             s.ext,
        no_macros:       s.no_macros,
        dry_run:         s.dry_run,
        db:              s.db,
        depfile:         s.depfile,
        stamp:           s.stamp,
        strict:          s.strict,
        warn_unused:     s.warn_unused,
        allow_env:       s.allow_env,
        allow_home:      s.allow_home,
        force_generated,
        sigil:           s.sigil,
        include:         s.include,
        formatter:       s.formatter,
        no_fts:          s.no_fts,
        dump_expanded:   s.dump_expanded,
    })?;
    Ok(())
}

fn main() {
    let cli = Cli::parse();

    let result: Result<(), Error> = if cli.single.directory.is_some() || !cli.single.inputs.is_empty() {
        run_single_pass_from_cli(cli.single, cli.force_generated)
    } else {
        run_multi_pass(&cli.config, cli.force_generated)
    };

    if let Err(e) = result {
        eprintln!("wb-tangle: {e}");
        std::process::exit(1);
    }
}
