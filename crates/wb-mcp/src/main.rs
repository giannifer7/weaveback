mod cli_generated;
use cli_generated::Cli;
use clap::Parser;
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
