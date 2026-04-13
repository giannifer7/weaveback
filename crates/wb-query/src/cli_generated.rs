use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Weaveback query: read-only analysis of the literate programming database.
#[derive(Parser, Debug)]
#[command(name = "wb-query", version)]
pub(crate) struct Cli {
    /// Path to the weaveback database.
    #[arg(long, default_value = "weaveback.db", global = true)]
    pub(crate) db: PathBuf,

    /// Base directory for generated output files (used for path resolution).
    #[arg(long = "gen", default_value = "gen", global = true)]
    pub(crate) gen_dir: PathBuf,

    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Map a generated file location to its literate source (noweb level).
    Where {
        /// Generated file path.
        out_file: String,
        /// Line number (1-indexed).
        line: u32,
    },
    /// Full trace: map a generated file location to its macro-level source.
    Trace {
        /// Generated file path.
        out_file: String,
        /// Line number (1-indexed).
        line: u32,
        /// Column number (1-indexed, 0 = start of line).
        #[arg(default_value = "0")]
        col: u32,
        /// Macro sigil character
    #[arg(long, default_value = "%")]

    sigil: char,
        /// Include paths for %include/%import (colon-separated on Unix)
    #[arg(long, default_value = ".")]

    include: String,
        /// Allow %env(NAME) to read environment variables
    #[arg(long)]

    allow_env: bool,
    },
    /// Compute transitive impact of changes to a chunk.
    Impact {
        /// Chunk name (e.g. "my-chunk" or "@file foo/bar.rs").
        chunk: String,
    },
    /// Export chunk dependency graph as Graphviz DOT.
    Graph {
        /// Restrict to the subgraph reachable from this chunk
    #[arg(long)]

    chunk: Option<String>,
    },
    /// List tagged source blocks.
    Tags {
        /// Filter to a single source file (plain relative path)
    #[arg(long)]

    file: Option<String>,
    },
    /// Lint literate source files.
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
    /// Map generated locations to their literate source (bulk mode).
    Attribute {
        /// Read plain text from stdin, extract FILE:LINE[:COL] locations, and attribute them
    #[arg(long)]

    scan_stdin: bool,
        /// Emit grouped source-of-truth summary JSON instead of a flat result array
    #[arg(long)]

    summary: bool,
        /// One or more generated locations in FILE:LINE or FILE:LINE:COL form

    locations: Vec<String>,
        /// Macro sigil character
    #[arg(long, default_value = "%")]

    sigil: char,
        /// Include paths for %include/%import (colon-separated on Unix)
    #[arg(long, default_value = ".")]

    include: String,
        /// Allow %env(NAME) to read environment variables
    #[arg(long)]

    allow_env: bool,
    },
    /// Report source coverage from an lcov file.
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
    /// Run cargo and annotate diagnostics with literate source locations.
    Cargo {
        /// Emit only compiler messages and the final weaveback summary, not Cargo artifact chatter
    #[arg(long)]

    diagnostics_only: bool,
        /// Cargo subcommand and arguments, passed through after `weaveback cargo`
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]

    args: Vec<String>,
        /// Macro sigil character
    #[arg(long, default_value = "%")]

    sigil: char,
        /// Include paths for %include/%import (colon-separated on Unix)
    #[arg(long, default_value = ".")]

    include: String,
        /// Allow %env(NAME) to read environment variables
    #[arg(long)]

    allow_env: bool,
    },
    /// Full-text search over tangled source content.
    Search {
        /// Search query (FTS5 syntax: AND, OR, NOT, phrase "...", prefix foo*)

    query: String,
        /// Maximum number of results to show
    #[arg(long, default_value = "10")]

    limit: usize,
    },
}
