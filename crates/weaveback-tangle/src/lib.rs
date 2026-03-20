pub mod db;
pub mod noweb;
pub mod safe_writer;

#[cfg(test)]
mod tests;

pub use noweb::ChunkError;

use db::DbError;
use safe_writer::SafeWriterError;
use std::fmt;

#[derive(Debug)]
pub enum WeavebackError {
    Chunk(ChunkError),
    SafeWriter(SafeWriterError),
    Db(DbError),
}

impl fmt::Display for WeavebackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WeavebackError::Chunk(e) => write!(f, "Chunk error: {}", e),
            WeavebackError::SafeWriter(e) => write!(f, "Safe writer error: {}", e),
            WeavebackError::Db(e) => write!(f, "Database error: {}", e),
        }
    }
}

impl std::error::Error for WeavebackError {}

impl From<ChunkError> for WeavebackError {
    fn from(err: ChunkError) -> Self {
        WeavebackError::Chunk(err)
    }
}

impl From<SafeWriterError> for WeavebackError {
    fn from(err: SafeWriterError) -> Self {
        WeavebackError::SafeWriter(err)
    }
}

impl From<DbError> for WeavebackError {
    fn from(err: DbError) -> Self {
        WeavebackError::Db(err)
    }
}

impl From<std::io::Error> for WeavebackError {
    fn from(err: std::io::Error) -> Self {
        WeavebackError::SafeWriter(SafeWriterError::IoError(err))
    }
}

pub use crate::db::{WeavebackDb, NowebMapEntry};
pub use crate::noweb::Clip;
pub use crate::safe_writer::SafeFileWriter;
pub use crate::safe_writer::SafeWriterConfig;
