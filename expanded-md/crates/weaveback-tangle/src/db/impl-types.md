# DB Types

Public DB error and record types.

## Error type and NowebMapEntry

```rust
// <[db-types]>=
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// How reliably a post-formatter output line was traced back to its source.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Confidence {
    /// Diff Equal match — the line survived formatting unchanged.
    #[default]
    Exact,
    /// Matched by normalised content hash — survives reordering (e.g. import sorting).
    HashMatch,
    /// Attribution inherited from the nearest attributed neighbour (gap-fill).
    Inferred,
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Confidence::Exact     => "exact",
            Confidence::HashMatch => "hash_match",
            Confidence::Inferred  => "inferred",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "hash_match" => Confidence::HashMatch,
            "inferred"   => Confidence::Inferred,
            _            => Confidence::Exact,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TangleConfig {
    pub sigil: char,
    pub open_delim: String,
    pub close_delim: String,
    pub chunk_end: String,
    pub comment_markers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NowebMapEntry {
    pub src_file: String,
    pub chunk_name: String,
    pub src_line: u32,
    pub indent: String,
    pub confidence: Confidence,
}

/// One parsed logical block stored in `source_blocks`.
#[derive(Debug, Clone)]
pub struct StoredBlockInfo {
    pub block_index:  u32,
    pub block_type:   String,
    pub line_start:   u32,
    pub line_end:     u32,
    pub content_hash: Vec<u8>,
}

/// Location of a chunk definition within a literate source file.
/// `def_start` is the 1-indexed line of the configured chunk open marker.
/// `def_end`   is the 1-indexed line of the configured chunk close marker.
#[derive(Debug, Clone)]
pub struct ChunkDefEntry {
    pub src_file:   String,
    pub chunk_name: String,
    pub nth:        u32,
    pub def_start:  u32,
    pub def_end:    u32,
}

/// A block that needs LLM tagging (either never tagged or content changed).
#[derive(Debug, Clone)]
pub struct BlockForTagging {
    pub src_file:    String,
    pub block_index: u32,
    pub block_type:  String,
    pub line_start:  u32,
    pub line_end:    u32,
    pub content_hash: Vec<u8>,
}
// @
```

