mod cli_generated;
use cli_generated::Cli;
use clap::Parser;
fn main() {
    let cli = Cli::parse();

    let backend = match cli.ai_backend.as_str() {
        "anthropic" => weaveback_serve::AiBackend::Anthropic,
        "gemini"    => weaveback_serve::AiBackend::Gemini,
        "ollama"    => weaveback_serve::AiBackend::Ollama,
        "openai"    => weaveback_serve::AiBackend::OpenAi,
        _           => weaveback_serve::AiBackend::ClaudeCli,
    };

    let tangle_cfg = weaveback_serve::TangleConfig {
        open_delim:      cli.open_delim,
        close_delim:     cli.close_delim,
        chunk_end:       cli.chunk_end,
        comment_markers: cli.comment_markers.split(',').map(|s| s.trim().to_string()).collect(),
        ai_backend:      backend,
        ai_model:        cli.ai_model,
        ai_endpoint:     cli.ai_endpoint,
    };

    if let Err(e) = weaveback_serve::run_serve(cli.port, cli.html, tangle_cfg, cli.watch) {
        eprintln!("wb-serve: {e}");
        std::process::exit(1);
    }
}
