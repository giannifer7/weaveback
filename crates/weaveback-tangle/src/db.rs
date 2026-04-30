// weaveback-tangle/src/db.rs
// I'd Really Rather You Didn't edit this generated file.

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::path::Path;

mod schema;
mod types;
mod open;
mod baselines;
mod noweb_map;
mod chunk_deps;
mod chunk_defs;
mod macro_map;
mod config;
mod source_blocks;
mod merge;
mod snapshots_defs;
mod fts;

pub use fts::{BlockForEmbedding, FtsResult, TaggedBlock};
pub use types::BlockForTagging;
pub use open::WeavebackDb;
pub use types::*;

pub(in crate::db) use open::{apply_schema, intern_file};
pub(in crate::db) use schema::CREATE_SCHEMA;

#[cfg(test)]
mod tests;

