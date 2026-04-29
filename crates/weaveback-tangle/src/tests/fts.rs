// weaveback-tangle/src/tests/fts.rs
// I'd Really Rather You Didn't edit this generated file.

mod prose_fts;
mod tags;
mod embeddings;
mod baselines;
mod noweb_map;
mod chunks;
mod config;
mod source_state;
mod block_status;

use crate::{
    block_parser::SourceBlockEntry,
    db::WeavebackDb,
};

/// Build a SourceBlockEntry for testing (hash doesn't matter for FTS tests).
fn block(index: u32, block_type: &str, line_start: u32, line_end: u32) -> SourceBlockEntry {
    SourceBlockEntry {
        block_index: index,
        block_type: block_type.to_string(),
        line_start,
        line_end,
        content_hash: [0u8; 32],
    }
}

