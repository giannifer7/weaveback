use clap::Parser;
use std::path::PathBuf;

/// Weaveback MCP server: JSON-RPC bridge for AI agent tooling.
///
/// Reads JSON-RPC 2.0 requests from stdin, writes responses to stdout.
/// Intended for use as a stdio-based MCP server.
#[derive(Parser, Debug)]
#[command(name = "wb-mcp", version)]
pub(crate) struct Cli {
    /// Path to the weaveback database.
    #[arg(long, default_value = "weaveback.db")]
    pub(crate) db: PathBuf,

    /// Base directory for generated output files.
    #[arg(long = "gen", default_value = "gen")]
    pub(crate) gen_dir: PathBuf,

        /// Macro sigil character
    #[arg(long, default_value = "%")]

    pub(crate) sigil: char,
        /// Include paths for %include/%import (colon-separated on Unix)
    #[arg(long, default_value = ".")]

    pub(crate) include: String,
        /// Allow %env(NAME) to read environment variables
    #[arg(long)]

    pub(crate) allow_env: bool,
}
