use clap::Parser;
use std::path::PathBuf;

/// Weaveback MCP server: JSON-RPC bridge for AI agent tooling.
///
/// Reads JSON-RPC 2.0 requests from stdin, writes responses to stdout.
/// Intended for use as a stdio-based MCP server.
#[derive(Parser, Debug)]
#[command(name = "wb-mcp", version)]
struct Cli {
    /// Path to the weaveback database.
    #[arg(long, default_value = "weaveback.db")]
    db: PathBuf,

    /// Base directory for generated output files.
    #[arg(long = "gen", default_value = "gen")]
    gen_dir: PathBuf,

    /// Macro sigil character.
    #[arg(long, default_value = "%")]
    sigil: char,

    /// Include paths for macro expansion (colon-separated on Unix).
    #[arg(long, default_value = ".")]
    include: String,

    /// Allow %env(NAME) builtins.
    #[arg(long)]
    allow_env: bool,
}
fn default_pathsep() -> String {
    if cfg!(windows) { ";".to_string() } else { ":".to_string() }
}

fn main() {
    let cli = Cli::parse();

    let pathsep = default_pathsep();
    let include_paths: Vec<std::path::PathBuf> = cli.include
        .split(&pathsep)
        .map(std::path::PathBuf::from)
        .collect();

    let eval_config = weaveback_macro::evaluator::EvalConfig {
        sigil: cli.sigil,
        include_paths,
        discovery_mode: false,
        allow_env: cli.allow_env,
    };

    if let Err(e) = weaveback_api::mcp::run_mcp(cli.db, cli.gen_dir, eval_config) {
        eprintln!("wb-mcp: {e}");
        std::process::exit(1);
    }
}
