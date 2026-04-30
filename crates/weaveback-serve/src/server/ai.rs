// weaveback-serve/src/server/ai.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

mod backends;
mod context;
mod handler;
mod stream;

use backends::{
    call_anthropic_api,
    call_claude_cli,
    call_gemini_api,
    call_ollama_api,
    call_openai_api,
};
pub(crate) use context::build_chunk_context;
pub(crate) use stream::{AiChannelReader, sse_headers};

pub(in crate::server) use handler::handle_ai;

#[cfg(test)]
pub(crate) use context::{
    dep_bodies,
    extract_prose,
    git_log_for_file,
    heading_level,
    section_range,
    title_chain,
};

