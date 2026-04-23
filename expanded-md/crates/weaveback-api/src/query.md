# Query API

Pure read-only query functions for chunk dependency analysis, graph
export, and tag listing.  All functions open the database themselves
from a path so callers do not need to manage `WeavebackDb` directly.

No I/O to stdout; callers decide how to present results.

## Error Type

```rust
// <[query-error]>=
use std::path::Path;
use weaveback_tangle::db::WeavebackDb;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    Db(#[from] weaveback_tangle::db::DbError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Open the weaveback SQLite database at `db_path` in read-only mode.
///
/// Returns a descriptive error if the file does not exist.
pub fn open_db(db_path: &Path) -> Result<WeavebackDb, ApiError> {
    if !db_path.exists() {
        return Err(ApiError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "Database not found at {}. Run weaveback on your source files first.",
                db_path.display()
            ),
        )));
    }
    Ok(WeavebackDb::open_read_only(db_path)?)
}
// @
```


## Impact Analysis

`impact_analysis` answers: _"if I change this chunk, what else is affected?"_

It performs a BFS forward through `chunk_deps` to collect all transitively
reachable chunks and the output files each chunk contributes to.

```rust
// <[query-impact]>=
/// Compute the transitive impact of changing `chunk`.
///
/// Returns a JSON object with fields:
/// - `chunk` — the root chunk name
/// - `reachable_chunks` — chunks that transitively depend on it
/// - `affected_files` — sorted list of generated files affected
pub fn impact_analysis(
    chunk: &str,
    db_path: &Path,
) -> Result<serde_json::Value, ApiError> {
    let db = open_db(db_path)?;

    let mut reachable: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    seen.insert(chunk.to_string());
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    queue.push_back(chunk.to_string());
    while let Some(current) = queue.pop_front() {
        for (child, _src_file) in db.query_chunk_deps(&current)? {
            if seen.insert(child.clone()) {
                reachable.push(child.clone());
                queue.push_back(child);
            }
        }
    }

    let mut affected: std::collections::HashSet<String> = std::collections::HashSet::new();
    for c in std::iter::once(&chunk.to_string()).chain(reachable.iter()) {
        for f in db.query_chunk_output_files(c)? {
            affected.insert(f);
        }
    }
    let mut affected_files: Vec<String> = affected.into_iter().collect();
    affected_files.sort();

    Ok(serde_json::json!({
        "chunk":            chunk,
        "reachable_chunks": reachable,
        "affected_files":   affected_files,
    }))
}
// @
```


## Chunk Dependency Graph

`chunk_graph_dot` renders the chunk dependency graph as Graphviz DOT
output.  Passing `chunk = Some(root)` limits the graph to the subgraph
reachable from `root`; `None` emits the full workspace graph.

```rust
// <[query-graph]>=
fn dot_id(name: &str) -> String {
    format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Render the chunk dependency graph as a Graphviz DOT string.
pub fn chunk_graph_dot(
    chunk: Option<&str>,
    db_path: &Path,
) -> Result<String, ApiError> {
    let db = open_db(db_path)?;

    let edges: Vec<(String, String)> = if let Some(root) = chunk {
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        visited.insert(root.to_string());
        queue.push_back(root.to_string());
        let mut sub: Vec<(String, String)> = Vec::new();
        while let Some(current) = queue.pop_front() {
            for (child, _) in db.query_chunk_deps(&current)? {
                sub.push((current.clone(), child.clone()));
                if visited.insert(child.clone()) {
                    queue.push_back(child);
                }
            }
        }
        sub
    } else {
        db.query_all_chunk_deps()?
            .into_iter()
            .map(|(f, t, _)| (f, t))
            .collect()
    };

    let mut out = String::from("digraph chunk_deps {\n");
    for (from, to) in &edges {
        out.push_str(&format!("  {} -> {};\n", dot_id(from), dot_id(to)));
    }
    out.push('}');
    Ok(out)
}
// @
```


## Tag Listing

`list_block_tags` returns all tagged source blocks, optionally filtered
to a single source file.

```rust
// <[query-tags]>=
/// List source blocks that have LLM-generated tags, optionally filtered by file.
pub fn list_block_tags(
    file: Option<&str>,
    db_path: &Path,
) -> Result<Vec<weaveback_tangle::db::TaggedBlock>, ApiError> {
    let db = WeavebackDb::open_read_only(db_path)
        .map_err(|e| ApiError::Io(std::io::Error::other(e.to_string())))?;
    Ok(db.list_block_tags(file)?)
}
// @
```


## Tests

The test body is generated as `query/tests.rs` and linked from
`query.rs` with `#[cfg(test)] mod tests;`.

```rust
// <[@file weaveback-api/src/query/tests.rs]>=
// weaveback-api/src/query/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use tempfile::TempDir;

fn make_db(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("weaveback.db");
    WeavebackDb::open(&path).expect("create test db");
    path
}

#[test]
fn dot_id_escapes_quotes() {
    assert_eq!(dot_id("a\"b"), "\"a\\\"b\"");
}

#[test]
fn dot_id_escapes_backslashes() {
    assert_eq!(dot_id("a\\b"), "\"a\\\\b\"");
}

#[test]
fn dot_id_plain_name_wrapped_in_quotes() {
    assert_eq!(dot_id("my-chunk"), "\"my-chunk\"");
}

#[test]
fn open_db_errors_when_file_missing() {
    let msg = open_db(Path::new("/nonexistent/path/weaveback.db"))
        .err()
        .expect("expected error")
        .to_string();
    assert!(
        msg.contains("not found") || msg.contains("Database not found"),
        "unexpected error: {msg}"
    );
}

#[test]
fn impact_analysis_returns_empty_for_unknown_chunk() {
    let dir = TempDir::new().unwrap();
    let path = make_db(&dir);
    let result = impact_analysis("nonexistent-chunk", &path).unwrap();
    assert_eq!(result["chunk"], "nonexistent-chunk");
    assert!(result["reachable_chunks"].as_array().unwrap().is_empty());
    assert!(result["affected_files"].as_array().unwrap().is_empty());
}

#[test]
fn chunk_graph_dot_returns_empty_graph_for_fresh_db() {
    let dir = TempDir::new().unwrap();
    let path = make_db(&dir);
    let dot = chunk_graph_dot(None, &path).unwrap();
    assert!(dot.starts_with("digraph chunk_deps {"));
    assert!(dot.ends_with('}'));
}

#[test]
fn chunk_graph_dot_with_chunk_filter_traverses_bfs() {
    let dir = TempDir::new().unwrap();
    let path = make_db(&dir);
    let mut db = WeavebackDb::open(&path).unwrap();
    db.set_chunk_deps(&[
        ("a".into(), "b".into(), "src.adoc".into()),
        ("b".into(), "c".into(), "src.adoc".into()),
        ("x".into(), "y".into(), "src.adoc".into()),
    ]).unwrap();
    drop(db);

    let dot = chunk_graph_dot(Some("a"), &path).unwrap();
    assert!(dot.contains("\"a\" -> \"b\""));
    assert!(dot.contains("\"b\" -> \"c\""));
    // x→y is in a disconnected subgraph and should be absent
    assert!(!dot.contains("\"x\""));
}

#[test]
fn chunk_graph_dot_none_includes_all_edges() {
    let dir = TempDir::new().unwrap();
    let path = make_db(&dir);
    let mut db = WeavebackDb::open(&path).unwrap();
    db.set_chunk_deps(&[
        ("a".into(), "b".into(), "src.adoc".into()),
        ("x".into(), "y".into(), "src.adoc".into()),
    ]).unwrap();
    drop(db);

    let dot = chunk_graph_dot(None, &path).unwrap();
    assert!(dot.contains("\"a\" -> \"b\""));
    assert!(dot.contains("\"x\" -> \"y\""));
}

#[test]
fn impact_analysis_traverses_transitive_deps() {
    let dir = TempDir::new().unwrap();
    let path = make_db(&dir);
    let mut db = WeavebackDb::open(&path).unwrap();
    db.set_chunk_deps(&[
        ("root".into(), "mid".into(),  "src.adoc".into()),
        ("mid".into(),  "leaf".into(), "src.adoc".into()),
    ]).unwrap();
    drop(db);

    let result = impact_analysis("root", &path).unwrap();
    let reachable: Vec<_> = result["reachable_chunks"]
        .as_array().unwrap()
        .iter().map(|v| v.as_str().unwrap()).collect();
    assert!(reachable.contains(&"mid"), "mid missing: {:?}", reachable);
    assert!(reachable.contains(&"leaf"), "leaf missing: {:?}", reachable);
}

#[test]
fn impact_analysis_avoids_infinite_loop_on_cycle() {
    let dir = TempDir::new().unwrap();
    let path = make_db(&dir);
    let mut db = WeavebackDb::open(&path).unwrap();
    db.set_chunk_deps(&[
        ("a".into(), "b".into(), "src.adoc".into()),
        ("b".into(), "a".into(), "src.adoc".into()),
    ]).unwrap();
    drop(db);

    // Must not hang or panic — cycles are valid in the dep graph
    let result = impact_analysis("a", &path).unwrap();
    assert!(result["reachable_chunks"].as_array().unwrap().contains(&serde_json::json!("b")));
}

#[test]
fn list_block_tags_returns_empty_for_fresh_db() {
    let dir = TempDir::new().unwrap();
    let path = make_db(&dir);
    let tags = list_block_tags(None, &path).unwrap();
    assert!(tags.is_empty());
}

#[test]
fn list_block_tags_with_filter_returns_empty_for_fresh_db() {
    let dir = TempDir::new().unwrap();
    let path = make_db(&dir);
    let tags = list_block_tags(Some("some/file.adoc"), &path).unwrap();
    assert!(tags.is_empty());
}

// @
```


## Assembly

```rust
// <[@file weaveback-api/src/query.rs]>=
// weaveback-api/src/query.rs
// I'd Really Rather You Didn't edit this generated file.

// <[query-error]>
// <[query-impact]>
// <[query-graph]>
// <[query-tags]>
#[cfg(test)]
mod tests;

// @
```

