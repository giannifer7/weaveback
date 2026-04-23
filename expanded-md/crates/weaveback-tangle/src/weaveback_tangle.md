# weaveback-tangle

`weaveback-tangle` reads literate source documents in noweb style, extracts
named code chunks, and writes the resulting files to disk atomically.  It is
the second stage of the weaveback pipeline — it receives the macro-expanded
text from `weaveback-macro` and produces the final generated source files.

## Public API

The crate exports five public types:

* `Clip` — high-level façade: read sources, expand chunks, write files.
* `SafeFileWriter` — atomic file writer with content-based modification detection.
* `SafeWriterConfig` — configuration struct for `SafeFileWriter`.
* `WeavebackDb` — SQLite-backed persistent store for baselines and source maps.
* `NowebMapEntry` — a single row of the `noweb_map` source-map table, mapping
  one output line back to its origin in the literate source.

`WeavebackError` is the unified error type wrapping the three sub-errors
produced by the crate's internal modules.

## Module map

The crate has four modules arranged in a chain:

....
CLI / Clip  (cli.adoc, noweb.adoc)
  └─▶ ChunkStore  parse chunk syntax; expand references recursively
  └─▶ ChunkWriter route @file output ─▶ SafeFileWriter or direct fs
         │
         ▼
  SafeFileWriter  (safe_writer.adoc)
      NamedTempFile staging, formatter, atomic copy, baseline check
         │
         ▼
  WeavebackDb  (db.adoc)
      gen_baselines · noweb_map · src_snapshots · var/macro_defs
....

<table>
  <tr><th>Module</th><th>Role</th></tr>
  <tr><td>[main.rs](cli.adoc)</td><td>CLI entry point — argument parsing, `--dry-run`, `--allow-home`, db merge</td></tr>
  <tr><td>[noweb.rs](noweb.adoc)</td><td>Parse chunk definitions, expand recursively, write via `ChunkWriter`,<br>
populate `noweb_map`</td></tr>
  <tr><td>[safe_writer.rs](safe_writer.adoc)</td><td>Atomic writes, content-based diffs, formatter hooks, modification detection</td></tr>
  <tr><td>[db.rs](db.adoc)</td><td>SQLite persistence: baselines, source maps, snapshots, definition spans</td></tr>
  <tr><td>[lookup.rs](lookup.adoc)</td><td>Source lookup and line tracing — shared by trace and apply-back</td></tr>
  <tr><td>[tests/](tests/tests.adoc)</td><td>Integration tests for all five modules</td></tr>
</table>

See [architecture.adoc](../../../docs/architecture.adoc) for the full
pipeline context, including apply-back and the MCP server.

## Error hierarchy

```text
WeavebackError
├── Chunk(ChunkError)            ← noweb.rs — parse/expand errors
├── SafeWriter(SafeWriterError)  ← safe_writer.rs — I/O, security, formatter
└── Db(DbError)                  ← db.rs — SQLite errors
```


`std::io::Error` converts to `WeavebackError::SafeWriter(SafeWriterError::IoError(_))`
so callers can use `?` on I/O operations without wrapping manually.

```rust
// <[@file weaveback-tangle/src/lib.rs]>=
// weaveback-tangle/src/lib.rs
// I'd Really Rather You Didn't edit this generated file.

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

// @
```

