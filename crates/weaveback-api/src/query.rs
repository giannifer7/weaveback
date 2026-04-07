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
/// List source blocks that have LLM-generated tags, optionally filtered by file.
pub fn list_block_tags(
    file: Option<&str>,
    db_path: &Path,
) -> Result<Vec<weaveback_tangle::db::TaggedBlock>, ApiError> {
    let db = WeavebackDb::open_read_only(db_path)
        .map_err(|e| ApiError::Io(std::io::Error::other(e.to_string())))?;
    Ok(db.list_block_tags(file)?)
}
#[cfg(test)]
mod tests {
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
}
