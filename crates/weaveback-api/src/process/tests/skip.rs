// weaveback-api/src/process/tests/skip.rs
// I'd Really Rather You Didn't edit this generated file.

use super::super::compute_skip_set;
use std::collections::HashMap;
use tempfile::tempdir;
use weaveback_tangle::db::WeavebackDb;

#[test]
fn compute_skip_set_with_no_prev_db_returns_empty() {
    let mut current_db = weaveback_tangle::db::WeavebackDb::open_temp().unwrap();
    let sources: HashMap<String, String> = HashMap::new();
    let skip = compute_skip_set(&sources, &None, &mut current_db, std::path::Path::new("/tmp"));
    assert!(skip.is_empty());
}
#[test]
fn compute_skip_set_propagates_dirty_chunks() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("prev.db");

    let mut prev_db = WeavebackDb::open(&db_path).unwrap();
    // Chunk B depends on Chunk A
    prev_db.set_chunk_defs(&[
        weaveback_tangle::db::ChunkDefEntry {
            src_file: "src.adoc".into(),
            chunk_name: "A".into(),
            nth: 0,
            def_start: 1,
            def_end: 10,
        },
        weaveback_tangle::db::ChunkDefEntry {
            src_file: "src.adoc".into(),
            chunk_name: "B".into(),
            nth: 0,
            def_start: 11,
            def_end: 20,
        },
    ]).unwrap();
    prev_db.set_chunk_deps(&[("B".into(), "A".into(), "src.adoc".into())]).unwrap();

    // Block 0 covers lines 1-10 (Chunk A)
    let block_a = weaveback_tangle::block_parser::SourceBlockEntry {
        block_index: 0,
        block_type: "code".into(),
        line_start: 1,
        line_end: 10,
        content_hash: [0u8; 32],
    };
    prev_db.set_source_blocks("src.adoc", std::slice::from_ref(&block_a)).unwrap();

    let mut current_db = WeavebackDb::open_temp().unwrap();
    let mut source_contents = HashMap::new();
    // Use content that will trigger the same block index but different hash
    // We'll mock the blocks directly because compute_skip_set calls parse_source_blocks
    // which we can't easily mock across crates without real content.
    source_contents.insert("src.adoc".to_string(), "<<A>>=\nnew content\n@".to_string());

    let skip = compute_skip_set(&source_contents, &Some(prev_db), &mut current_db, tmp.path());

    // Since original blocks were [1,2,3] and new will be different,
    // Chunk A becomes dirty, and Chunk B becomes dirty via reverse deps.
    assert!(!skip.contains("A"));
    assert!(!skip.contains("B"));
}
#[test]
fn test_compute_skip_set_with_dependencies() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("wb.db");
    let mut db = weaveback_tangle::db::WeavebackDb::open(&db_path).unwrap();

    let path = "test.adoc";
    let content = "base content";
    let blocks = weaveback_tangle::parse_source_blocks(content, "adoc");
    db.set_source_blocks(path, &blocks).unwrap();
    db.set_chunk_defs(&[weaveback_tangle::db::ChunkDefEntry {
        src_file: path.to_string(),
        chunk_name: "base".to_string(),
        nth: 0,
        def_start: 1,
        def_end: 1,
    }]).unwrap();
    db.set_chunk_deps(&[("dep".to_string(), "base".to_string(), path.to_string())]).unwrap();

    let mut source_contents = HashMap::new();
    source_contents.insert(path.to_string(), "changed content".to_string());

    let mut current_db = weaveback_tangle::db::WeavebackDb::open_temp().unwrap();
    let skip_set = compute_skip_set(&source_contents, &Some(db), &mut current_db, tmp.path());

    // "base" is dirty because content changed.
    // "dep" should be dirty via reverse dependency.
    assert!(!skip_set.contains("base"));
    assert!(!skip_set.contains("dep"));
}

