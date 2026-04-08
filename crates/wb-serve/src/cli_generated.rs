use clap::Parser;
use std::path::PathBuf;



/// Weaveback serve: live-reload documentation server with AI panel.
#[derive(Parser, Debug)]
#[command(name = "wb-serve", version)]
pub(crate) struct Cli {


        /// TCP port to listen on
    #[arg(long, default_value = "7779")]

    pub(crate) port: u16,



        /// Directory to serve (default: <project-root>/docs/html)
    #[arg(long)]

    pub(crate) html: Option<PathBuf>,



        /// Chunk open delimiter for the tangle oracle (default: <[)
    #[arg(long, default_value = "<[")]

    pub(crate) open_delim: String,



        /// Chunk close delimiter for the tangle oracle (default: ]>)
    #[arg(long, default_value = "]>")]

    pub(crate) close_delim: String,



        /// Chunk-end marker for the tangle oracle (default: @@)
    #[arg(long, default_value = "@@")]

    pub(crate) chunk_end: String,



        /// Comment markers for the tangle oracle (comma-separated, default: //)
    #[arg(long, default_value = "//")]

    pub(crate) comment_markers: String,



        /// AI backend for /__ai: "claude-cli" (default), "anthropic", "gemini", "ollama", "openai"
    #[arg(long, default_value = "claude-cli")]

    pub(crate) ai_backend: String,



        /// AI model name (e.g. "claude-3-5-sonnet-20240620", "gemini-1.5-pro", "llama3")
    #[arg(long)]

    pub(crate) ai_model: Option<String>,



        /// AI API endpoint / base URL (for ollama or openai-compatible backends)
    #[arg(long)]

    pub(crate) ai_endpoint: Option<String>,



        /// Watch .adoc and theme sources; tangle + re-render docs on each change
    #[arg(long)]

    pub(crate) watch: bool,



}
