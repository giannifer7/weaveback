pub mod block_parser;
pub mod db;
pub mod noweb;
pub mod safe_writer;
pub mod lookup;

#[cfg(test)]
mod tests;

pub use noweb::ChunkError;

use db::DbError;
use safe_writer::SafeWriterError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WeavebackError {
    #[error("Chunk error: {0}")]
    Chunk(#[from] ChunkError),
    #[error("Safe writer error: {0}")]
    SafeWriter(#[from] SafeWriterError),
    #[error("Database error: {0}")]
    Db(#[from] DbError),
}

impl From<std::io::Error> for WeavebackError {
    fn from(err: std::io::Error) -> Self {
        WeavebackError::SafeWriter(SafeWriterError::IoError(err))
    }
}

pub use crate::block_parser::{parse_source_blocks, SourceBlockEntry};
pub use crate::db::{WeavebackDb, NowebMapEntry, ChunkDefEntry, FtsResult, BlockForTagging, TaggedBlock};
pub use crate::noweb::{ChunkDefinitionMatch, Clip, NowebSyntax, tangle_check};
pub use crate::safe_writer::SafeFileWriter;
pub use crate::safe_writer::SafeWriterConfig;
