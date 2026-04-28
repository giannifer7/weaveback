# Serve Tangle Configuration

Runtime tangle configuration and tangle-oracle validation.

## Tangle configuration

`TangleConfig` carries the chunk-syntax parameters for the in-memory tangle
oracle used by `/__apply`.  The defaults match the weaveback project's own
conventions (`<[` / `]>` / `@@` / `//`).

```rust
// <[serve-tangle-config]>=
/// Which backend `/__ai` uses to answer questions.
#[derive(Clone, Debug)]
pub enum AiBackend {
    /// Shells out to `claude -p --output-format stream-json`.
    /// Uses the existing Claude Code session; no API key required.
    ClaudeCli,
    /// Calls the Anthropic API directly via HTTP.
    /// Requires the `ANTHROPIC_API_KEY` environment variable.
    Anthropic,
    /// Calls the Google Gemini API directly via HTTP.
    /// Requires the `GOOGLE_API_KEY` environment variable.
    Gemini,
    /// Calls a local Ollama API via HTTP.
    Ollama,
    /// Calls an OpenAI-compatible API via HTTP.
    /// Requires the `OPENAI_API_KEY` environment variable (if not using a local provider).
    OpenAi,
}

pub struct TangleConfig {
    pub open_delim:      String,
    pub close_delim:     String,
    pub chunk_end:       String,
    pub comment_markers: Vec<String>,
    pub ai_backend:      AiBackend,
    pub ai_model:        Option<String>,
    pub ai_endpoint:     Option<String>,
}

impl Default for TangleConfig {
    fn default() -> Self {
        Self {
            open_delim:      "<[".into(),
            close_delim:     "]>".into(),
            chunk_end:       "@@".into(),
            comment_markers: vec!["//".into()],
            ai_backend:      AiBackend::ClaudeCli,
            ai_model:        None,
            ai_endpoint:     None,
        }
    }
}
// @
```

