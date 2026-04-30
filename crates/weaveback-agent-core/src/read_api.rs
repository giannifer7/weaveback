// weaveback-agent-core/src/read_api.rs
// I'd Really Rather You Didn't edit this generated file.

use crate::workspace::WorkspaceConfig;
use serde::{Deserialize, Serialize};
use weaveback_core::PathResolver;
use weaveback_macro::evaluator::output::{PreciseTracingOutput, SourceSpan, SpanKind, SpanRange};
use weaveback_macro::evaluator::{EvalConfig, Evaluator};
use weaveback_macro::macro_api::process_string_precise;
use weaveback_tangle::db::WeavebackDb;
use weaveback_tangle::lookup::{find_best_noweb_entry, find_line_col};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub src_file: String,
    pub block_type: String,
    pub line_start: usize,
    pub line_end: usize,
    pub snippet: String,
    pub tags: Vec<String>,
    pub score: f64,
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResult {
    pub out_file: String,
    pub out_line: u32,
    pub src_file: Option<String>,
    pub src_line: Option<u32>,
    pub src_col: Option<u32>,
    pub kind: Option<String>,
    pub macro_name: Option<String>,
    pub param_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkContext {
    pub file: String,
    pub name: String,
    pub nth: u32,
    pub section_breadcrumb: Vec<String>,
    pub prose: String,
    pub body: String,
    pub direct_dependencies: Vec<String>,
    pub outputs: Vec<String>,
}
mod context;
mod db;
mod search;
mod trace;

pub use context::chunk_context;
pub use search::search;
pub use trace::trace;

#[cfg(test)]
use context::{extract_prose, heading_level, section_range, title_chain};
#[cfg(test)]
use search::{prepare_fts_query, reciprocal_rank};

#[cfg(test)]
mod tests;

