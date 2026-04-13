use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "weaveback",
    version,
    about = "Macro expander + literate-programming chunk extractor as one command"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Commands>,

    #[command(flatten)]
    pub(crate) args: Args,
}

#[derive(clap::Subcommand, Debug)]
pub(crate) enum Commands {
    /// Trace back output line to its noweb and macro sources
    Trace {
        out_file: String,
        line: u32,
        /// 1-indexed character position within the output line.
        /// Defaults to 1 (first character). Use this to look past a structural
        /// wrapper and find the token that produced a specific sub-expression.
        #[arg(long, default_value = "1")]
        col: u32,
    },
    /// Find the noweb chunk that produced output line
    Where {
        out_file: String,
        line: u32,
    },
    /// Run all tangle passes from weaveback.toml (or --config <file>)
    Tangle {
        /// Path to the tangle config file
    #[arg(long, default_value = "weaveback.toml")]

    config: std::path::PathBuf,
        /// Overwrite generated files even if they differ from the stored baseline.
        /// Use this only when the literate source is the authoritative state.
    #[arg(long)]

    force_generated: bool,
    },
    /// Parse FILE:LINE[:COL] and return structured source-of-truth attribution
    Attribute {
        /// Read plain text from stdin, extract FILE:LINE[:COL] locations, and attribute them
    #[arg(long)]

    scan_stdin: bool,
        /// Emit grouped source-of-truth summary JSON instead of a flat result array
    #[arg(long)]

    summary: bool,
        /// One or more generated locations in FILE:LINE or FILE:LINE:COL form

    locations: Vec<String>,
    },
    /// Run cargo with JSON diagnostics and add source-of-truth attribution
    Cargo {
        /// Emit only compiler messages and the final weaveback summary, not Cargo artifact chatter
    #[arg(long)]

    diagnostics_only: bool,
        /// Cargo subcommand and arguments, passed through after `weaveback cargo`
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]

    args: Vec<String>,
    },
    /// Tag prose blocks with LLM-generated tags, then rebuild the FTS index
    Tag {
        /// Path to the tangle config file (reads [tags] section)
    #[arg(long, default_value = "weaveback.toml")]

    config: std::path::PathBuf,
        /// Override backend (anthropic/gemini/openai/ollama)
    #[arg(long)]

    backend: Option<String>,
        /// Override model name
    #[arg(long)]

    model: Option<String>,
        /// Override API endpoint (for ollama / openai-compatible)
    #[arg(long)]

    endpoint: Option<String>,
        /// Override blocks per LLM request
    #[arg(long)]

    batch_size: Option<usize>,
    },
    /// Run as an MCP server for IDE/agent integration
    Mcp,
    /// Propagate edits in gen/ back to the literate source
    ApplyBack {
        /// Relative paths within gen/ to process (default: all modified files)

    files: Vec<String>,
        /// Show what would change without writing anything
    #[arg(long)]

    dry_run: bool,
    },
    /// Show every chunk and output file transitively affected if CHUNK changes
    Impact {
        chunk: String,
    },
    /// Export the chunk-dependency graph in DOT (Graphviz) format
    Graph {
        /// Restrict to the subgraph reachable from this chunk
    #[arg(long)]

    chunk: Option<String>,
    },
    /// Search literate source prose (FTS5 + tags + optional embeddings)
    Search {
        /// Search query (FTS5 syntax: AND, OR, NOT, phrase "...", prefix foo*)

    query: String,
        /// Maximum number of results to show
    #[arg(long, default_value = "10")]

    limit: usize,
    },
    /// Run structural checks on literate source files
    Lint {
        /// Files or directories to lint (default: current tree)

    paths: Vec<PathBuf>,
        /// Treat violations as errors
    #[arg(long)]

    strict: bool,
        /// Restrict linting to one rule
    #[arg(long)]

    rule: Option<String>,
        /// Emit structured JSON instead of human-readable text
    #[arg(long)]

    json: bool,
    },
    /// Regroup LCOV line coverage by owning literate source and section
    Coverage {
        /// Print a concise human summary ranked by missed lines instead of full JSON
    #[arg(long)]

    summary: bool,
        /// Maximum number of source files to show in summary mode
    #[arg(long, default_value = "10")]

    top_sources: usize,
        /// Maximum number of sections to show per source file in summary mode
    #[arg(long, default_value = "3")]

    top_sections: usize,
        /// For each unattributed file show the unmapped line ranges with source content
    #[arg(long)]

    explain_unattributed: bool,
        /// Path to an LCOV tracefile, typically `lcov.info` from `cargo llvm-cov --lcov`

    lcov_file: PathBuf,
    },
    /// List LLM-generated tags for prose blocks
    Tags {
        /// Filter to a single source file (plain relative path)
    #[arg(long)]

    file: Option<String>,
    },
    /// Semantic language server operations (requires rust-analyzer)
    Lsp {
        /// Manual override for the LSP command (e.g. "nimlsp")
    #[arg(long)]

    lsp_cmd: Option<String>,
        /// Manual override for the language ID (e.g. "nim")
    #[arg(long)]

    lsp_lang: Option<String>,
        #[command(subcommand)]
        cmd: LspCommands,
    },
    /// Serve docs/html/ locally with live reload and "Edit source" navigation
    Serve {
        /// TCP port to listen on
    #[arg(long, default_value = "7779")]

    port: u16,
        /// Directory to serve (default: <project-root>/docs/html)
    #[arg(long)]

    html: Option<PathBuf>,
        /// Chunk open delimiter for the tangle oracle (default: <[)
    #[arg(long, default_value = "<[")]

    open_delim: String,
        /// Chunk close delimiter for the tangle oracle (default: ]>)
    #[arg(long, default_value = "]>")]

    close_delim: String,
        /// Chunk-end marker for the tangle oracle (default: @@)
    #[arg(long, default_value = "@@")]

    chunk_end: String,
        /// Comment markers for the tangle oracle (comma-separated, default: //)
    #[arg(long, default_value = "//")]

    comment_markers: String,
        /// AI backend for /__ai: "claude-cli" (default), "anthropic", "gemini", "ollama", "openai"
    #[arg(long, default_value = "claude-cli")]

    ai_backend: String,
        /// AI model name (e.g. "claude-3-5-sonnet-20240620", "gemini-1.5-pro", "llama3")
    #[arg(long)]

    ai_model: Option<String>,
        /// AI API endpoint / base URL (for ollama or openai-compatible backends)
    #[arg(long)]

    ai_endpoint: Option<String>,
        /// Watch .adoc and theme sources; tangle + re-render docs on each change
    #[arg(long)]

    watch: bool,
    },
}

#[derive(clap::Subcommand, Debug)]
pub(crate) enum LspCommands {
    /// Go to definition of a symbol and map it to literate source
    Definition {
        out_file: String,
        line: u32,
        col: u32,
    },
    /// Find all references to a symbol and map them to literate sources
    References {
        out_file: String,
        line: u32,
        col: u32,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct Args {
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
    /// Overwrite generated files even if they differ from the stored baseline.
    /// Intended for explicit recovery or regeneration workflows.
    #[arg(long)]

    pub(crate) force_generated: bool,
    /// Discover and process driver files under this directory.
    /// A driver is any file (matching --ext) not referenced by a %include() in another such file.
    /// Mutually exclusive with positional input files.
    #[arg(long = "dir", conflicts_with = "inputs")]

    pub(crate) directory: Option<PathBuf>,
    /// File extension(s) to scan in --dir mode (can be repeated).
    /// Default: md. Example: --ext adoc --ext md to scan both.
    #[arg(long, default_value = "md")]

    pub(crate) ext: Vec<String>,
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
