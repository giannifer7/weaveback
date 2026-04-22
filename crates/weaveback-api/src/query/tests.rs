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

