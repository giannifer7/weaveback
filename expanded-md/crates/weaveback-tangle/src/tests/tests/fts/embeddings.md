# Embedding Search





```rust
// <[@file weaveback-tangle/src/tests/fts/embeddings.rs]>=
// weaveback-tangle/src/tests/fts/embeddings.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

#[test]
fn test_get_blocks_needing_embeddings_returns_all_when_none_stored() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= Section\n\nParagraph.\n";
    db.set_src_snapshot("a.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks("a.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();

    let need = db.get_blocks_needing_embeddings("test-model").unwrap();
    assert_eq!(need.len(), 2);
}

#[test]
fn test_get_blocks_needing_embeddings_respects_hash_and_model() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= Section\n\nParagraph.\n";
    db.set_src_snapshot("a.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks("a.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();

    db.set_block_embedding("a.adoc", 0, &[0u8; 32], "model-a", &[1.0, 0.0]).unwrap();
    db.set_block_embedding("a.adoc", 1, &[9u8; 32], "model-a", &[0.0, 1.0]).unwrap();

    let need_same_model = db.get_blocks_needing_embeddings("model-a").unwrap();
    assert_eq!(need_same_model.len(), 1);
    assert_eq!(need_same_model[0].block_index, 1);

    let need_new_model = db.get_blocks_needing_embeddings("model-b").unwrap();
    assert_eq!(need_new_model.len(), 2);
}

#[test]
fn test_search_prose_by_embedding_ranks_semantic_match() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= Search\n\nApples and fruit.\n\nOranges and citrus.\n";
    db.set_src_snapshot("a.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks("a.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
        block(2, "para",    5, 5),
    ]).unwrap();
    db.set_block_tags("a.adoc", 1, &[0u8; 32], "fruit").unwrap();
    db.set_block_tags("a.adoc", 2, &[0u8; 32], "citrus").unwrap();
    db.set_block_embedding("a.adoc", 1, &[0u8; 32], "model-a", &[1.0, 0.0]).unwrap();
    db.set_block_embedding("a.adoc", 2, &[0u8; 32], "model-a", &[0.0, 1.0]).unwrap();

    let results = db.search_prose_by_embedding(&[0.9, 0.1], 10).unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].line_start, 3);
    assert_eq!(results[0].tags, "fruit");
}

// @@
```

