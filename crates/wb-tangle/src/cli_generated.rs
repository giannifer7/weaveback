use clap::{Parser, Args};
use std::path::PathBuf;

/// Weaveback tangle: literate programming build tool.
///
/// With no --dir flag: reads weaveback.toml and runs all [[pass]] entries.
/// With --dir DIR: single-pass mode — processes one directory of .adoc files.
#[derive(Parser, Debug)]
#[command(name = "wb-tangle", version)]
pub(crate) struct Cli {
        /// Path to the tangle config file
    #[arg(long, default_value = "weaveback.toml")]

    pub(crate) config: std::path::PathBuf,
        /// Overwrite generated files even if they differ from the stored baseline.
        /// Use this only when the literate source is the authoritative state.
    #[arg(long)]

    pub(crate) force_generated: bool,

    #[command(flatten)]
    pub(crate) single: SinglePassCli,
}

/// All single-pass flags (used when --dir is present).
#[derive(Args, Debug)]
pub(crate) struct SinglePassCli {
    /// Input files (mutually exclusive with --dir)
    #[arg(required = false)]

    pub(crate) inputs: Vec<PathBuf>,
    /// Base directory prepended to every input path
    #[arg(long, default_value = ".")]

    pub(crate) input_dir: PathBuf,
    /// Macro sigil
    #[arg(long, default_value = "%")]

    pub(crate) sigil: char,
    /// Skip macro expansion and feed source files directly to the tangle pass.
    /// Use this when the source files contain no macros and the sigil
    /// character would collide with literal text (e.g. %, ^ in Rust or shell).
    #[arg(long)]

    pub(crate) no_macros: bool,
    /// Include paths for %include/%import (colon-separated on Unix)
    #[arg(long, default_value = ".")]

    pub(crate) include: String,
    /// Path to the weaveback database [default: weaveback.db in current directory]
    #[arg(long, default_value = "weaveback.db")]

    pub(crate) db: PathBuf,
    /// Print macro-expanded text to stderr before noweb processing
    #[arg(long)]

    pub(crate) dump_expanded: bool,
    /// Discover and process driver files under this directory.
    /// A driver is any file (matching --ext) not referenced by a %include() in another such file.
    /// Mutually exclusive with positional input files.
    #[arg(long = "dir", conflicts_with = "inputs")]

    pub(crate) directory: Option<PathBuf>,
    /// File extension(s) to scan in --dir mode (can be repeated).
    /// Default: md. Example: --ext adoc --ext md to scan both.
    #[arg(long, default_value = "md")]

    pub(crate) ext: Vec<String>,
    /// Base directory for generated output files
    #[arg(long = "gen", default_value = "gen")]

    pub(crate) gen_dir: PathBuf,
    /// Chunk open delimiter
    #[arg(long, default_value = "<[")]

    pub(crate) open_delim: String,
    /// Chunk close delimiter
    #[arg(long, default_value = "]>")]

    pub(crate) close_delim: String,
    /// Chunk end marker
    #[arg(long, default_value = "@")]

    pub(crate) chunk_end: String,
    /// Comment markers recognised before chunk delimiters (comma-separated)
    #[arg(long, default_value = "#,//")]

    pub(crate) comment_markers: String,
    /// Formatter command per output file extension, e.g. --formatter rs=rustfmt
    #[arg(long, value_name = "EXT=CMD")]

    pub(crate) formatter: Vec<String>,
    /// Write a Makefile depfile listing every source file read.
    /// In --dir mode the depfile lists ALL matching files found so that
    /// adding a new file triggers a rebuild.
    #[arg(long)]

    pub(crate) depfile: Option<PathBuf>,
    /// Touch this file on success (build-system stamp).
    #[arg(long)]

    pub(crate) stamp: Option<PathBuf>,
    /// Skip rebuilding the prose full-text search index after this run.
    /// Used internally by `weaveback tangle` to avoid concurrent FTS rebuilds;
    /// the tangle command rebuilds the index once after all passes complete.
    #[arg(long, hide = true)]

    pub(crate) no_fts: bool,
    /// Allow %env(NAME) to read environment variables.
    /// Disabled by default to prevent templates from silently reading secrets.
    #[arg(long)]

    pub(crate) allow_env: bool,
    /// Allow @file ~/… chunks to write outside the gen/ directory.
    #[arg(long)]

    pub(crate) allow_home: bool,
    /// Treat references to undefined chunks as fatal errors (default: expand to nothing).
    #[arg(long)]

    pub(crate) strict: bool,
    /// Print output paths without writing anything.
    #[arg(long)]

    pub(crate) dry_run: bool,
    /// Warn about chunks that are defined but never referenced by any @file chunk.
    /// Suppressed by default to keep output clean when large libraries of helper
    /// chunks are defined speculatively.
    #[arg(long)]

    pub(crate) warn_unused: bool,
}
