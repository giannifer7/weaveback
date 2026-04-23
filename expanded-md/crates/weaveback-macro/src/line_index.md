# Line index — byte offset to (line, column)

`line_index.rs` provides `LineIndex`: a small helper that converts absolute
byte offsets into 1-indexed `(line, column)` pairs.  It is used by the
evaluator and CLI to produce human-readable error locations.

## Design rationale

### Build once, query many times

Construct `LineIndex::new(source)` once per source string.  The constructor
scans the bytes with `memchr::memchr_iter` — the fastest SIMD newline scanner
available on x86/ARM — and stores a sorted `Vec<usize>` of newline positions.
Each `line_col` query does a single `partition_point` (binary search): O(log n)
per lookup, O(n) construction.

### `from_bytes` companion

`from_bytes` accepts a byte slice so callers that already hold `&[u8]` (e.g.
the safe writer comparing file contents) do not need a UTF-8 conversion round-trip.

### 1-indexed output

`line_col` returns 1-indexed `(line, column)` to match conventional editor
and compiler output formats.  Column is a byte offset within the line, not a
Unicode character count — consistent with how the rest of the pipeline treats
source positions.

## File structure

```rust
// <[@file weaveback-macro/src/line_index.rs]>=
// weaveback-macro/src/line_index.rs
// I'd Really Rather You Didn't edit this generated file.

// <[line index preamble]>
// <[line index struct]>
#[cfg(test)]
mod tests;

// @
```


## Preamble

```rust
// <[line index preamble]>=
use memchr::memchr_iter;
// @
```


## `LineIndex`

```rust
// <[line index struct]>=
/// Converts byte offsets to 1-indexed (line, column) pairs on demand.
///
/// Construct once per source string; each lookup is O(log n) via binary search
/// over the cached newline positions.
pub struct LineIndex {
    /// Sorted byte offsets of every `\n` in the source.
    newlines: Vec<usize>,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let newlines = memchr_iter(b'\n', source.as_bytes()).collect();
        Self { newlines }
    }

    pub fn from_bytes(source: &[u8]) -> Self {
        let newlines = memchr_iter(b'\n', source).collect();
        Self { newlines }
    }

    /// Returns the 1-indexed `(line, column)` for a byte offset.
    /// Column is a 1-indexed byte offset within the line.
    pub fn line_col(&self, pos: usize) -> (usize, usize) {
        // Number of newlines strictly before `pos` = the line index (0-based).
        let i = self.newlines.partition_point(|&nl| nl < pos);
        let line_start = if i == 0 { 0 } else { self.newlines[i - 1] + 1 };
        (i + 1, pos - line_start + 1)
    }
}
// @
```


## Tests

The tests are generated as `line_index/tests.rs` and linked from the runtime
module with `#[cfg(test)] mod tests;`.  This keeps runtime code short while
preserving local literate ownership.

```rust
// <[@file weaveback-macro/src/line_index/tests.rs]>=
// weaveback-macro/src/line_index/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::LineIndex;

#[test]
fn test_single_line() {
    let idx = LineIndex::new("hello");
    assert_eq!(idx.line_col(0), (1, 1));
    assert_eq!(idx.line_col(4), (1, 5));
}

#[test]
fn test_two_lines() {
    // "ab\ncd"  — newline at offset 2
    let idx = LineIndex::new("ab\ncd");
    assert_eq!(idx.line_col(0), (1, 1)); // 'a'
    assert_eq!(idx.line_col(1), (1, 2)); // 'b'
    assert_eq!(idx.line_col(2), (1, 3)); // '\n'
    assert_eq!(idx.line_col(3), (2, 1)); // 'c'
    assert_eq!(idx.line_col(4), (2, 2)); // 'd'
}

#[test]
fn test_three_lines() {
    let idx = LineIndex::new("a\nb\nc");
    assert_eq!(idx.line_col(0), (1, 1));
    assert_eq!(idx.line_col(2), (2, 1));
    assert_eq!(idx.line_col(4), (3, 1));
}

// @
```

