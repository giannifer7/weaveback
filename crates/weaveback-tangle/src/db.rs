// weaveback-tangle/src/db.rs
// I'd Really Rather You Didn't edit this generated file.

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::path::Path;

include!("db/schema.rs");
include!("db/types.rs");
include!("db/open.rs");
include!("db/baselines.rs");
include!("db/noweb_map.rs");
include!("db/chunk_deps.rs");
include!("db/chunk_defs.rs");
include!("db/macro_map.rs");
include!("db/config.rs");
include!("db/source_blocks.rs");
include!("db/merge.rs");
include!("db/snapshots_defs.rs");
include!("db/fts.rs");

#[cfg(test)]
mod tests;

