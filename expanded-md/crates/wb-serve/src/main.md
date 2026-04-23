# wb-serve

`wb-serve` is the weaveback HTTP documentation server.  It serves the
pre-generated HTML docs with live reload, an AI panel for chunk context,
and apply-back for propagating edits back to literate sources.

## CLI

Generated from `cli-spec/wb-serve-cli.adoc`.

```rust
// <[wb-serve-cli]>=
mod cli_generated;
use cli_generated::Cli;
use clap::Parser;
// @
```


## Main

```rust
// <[wb-serve-main]>=
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
// @
```


## Assembly

```rust
// <[@file wb-serve/src/main.rs]>=
// wb-serve/src/main.rs
// I'd Really Rather You Didn't edit this generated file.

// <[wb-serve-cli]>
// <[wb-serve-main]>

// @
```

