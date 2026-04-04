// src/tests/fts.rs
use crate::{
    block_parser::SourceBlockEntry,
    db::WeavebackDb,
};

/// Build a SourceBlockEntry for testing (hash doesn't matter for FTS tests).
fn block(index: u32, block_type: &str, line_start: u32, line_end: u32) -> SourceBlockEntry {
    SourceBlockEntry {
        block_index: index,
        block_type: block_type.to_string(),
        line_start,
        line_end,
        content_hash: [0u8; 32],
    }
}

#[test]
fn test_fts_basic_search() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= Introduction\n\nWeaveback is a literate programming tool.\n";
    db.set_src_snapshot("docs/intro.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks("docs/intro.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();

    db.rebuild_prose_fts().unwrap();

    let results = db.search_prose("literate", 10).unwrap();
    assert!(!results.is_empty(), "should find 'literate' in para block");
    assert_eq!(results[0].src_file, "docs/intro.adoc");
    assert_eq!(results[0].block_type, "para");
}

#[test]
fn test_fts_section_and_para_indexed_code_not() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= My Section\n\nSome prose here.\n\n----\nfn secret_fn() {}\n----\n";
    db.set_src_snapshot("src/lib.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks("src/lib.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
        block(2, "code",    5, 7),
    ]).unwrap();

    db.rebuild_prose_fts().unwrap();

    let r = db.search_prose("prose", 10).unwrap();
    assert!(!r.is_empty());

    // code content is NOT indexed
    let r = db.search_prose("secret_fn", 10).unwrap();
    assert!(r.is_empty(), "code blocks should not be indexed");
}

#[test]
fn test_fts_multiple_files() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let src_a = "= Alpha\n\nThis covers the alpha feature.\n";
    let src_b = "= Beta\n\nThis covers the beta feature.\n";

    db.set_src_snapshot("docs/alpha.adoc", src_a.as_bytes()).unwrap();
    db.set_source_blocks("docs/alpha.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();

    db.set_src_snapshot("docs/beta.adoc", src_b.as_bytes()).unwrap();
    db.set_source_blocks("docs/beta.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();

    db.rebuild_prose_fts().unwrap();

    let r = db.search_prose("feature", 10).unwrap();
    let files: Vec<&str> = r.iter().map(|x| x.src_file.as_str()).collect();
    assert!(files.contains(&"docs/alpha.adoc"));
    assert!(files.contains(&"docs/beta.adoc"));

    // "alpha feature" is unique to alpha.adoc
    let r = db.search_prose("\"alpha feature\"", 10).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].src_file, "docs/alpha.adoc");
}

#[test]
fn test_fts_rebuild_is_idempotent() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= Title\n\nSome searchable content.\n";
    db.set_src_snapshot("a.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks("a.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();

    db.rebuild_prose_fts().unwrap();
    db.rebuild_prose_fts().unwrap();

    let r = db.search_prose("searchable", 10).unwrap();
    assert_eq!(r.len(), 1, "duplicate rebuild must not produce duplicate results");
}

#[test]
fn test_fts_empty_source_no_panic() {
    let mut db = WeavebackDb::open_temp().unwrap();
    db.set_src_snapshot("empty.adoc", b"").unwrap();
    // No source_blocks for this file — must not panic or error.
    db.rebuild_prose_fts().unwrap();
    let r = db.search_prose("anything", 10).unwrap();
    assert!(r.is_empty());
}

#[test]
fn test_fts_normalises_dotslash_path() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= Normalised\n\nThis tests path normalisation.\n";
    // Snapshot stored with "./" prefix (as some passes do).
    db.set_src_snapshot("./docs/norm.adoc", source.as_bytes()).unwrap();
    // source_blocks stored under the plain path (as the files table has it).
    db.set_source_blocks("docs/norm.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();

    db.rebuild_prose_fts().unwrap();

    let r = db.search_prose("normalisation", 10).unwrap();
    assert!(!r.is_empty(), "dotslash prefix must be stripped before path lookup");
}

#[test]
fn test_fts_deduplicates_same_path_stored_twice() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= Dedup\n\nThis text should appear once.\n";
    // Same file stored under two keys (as two passes might do).
    db.set_src_snapshot("./docs/dedup.adoc", source.as_bytes()).unwrap();
    db.set_src_snapshot("docs/dedup.adoc",   source.as_bytes()).unwrap();
    db.set_source_blocks("docs/dedup.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();

    db.rebuild_prose_fts().unwrap();

    let r = db.search_prose("once", 10).unwrap();
    assert_eq!(r.len(), 1, "same file stored twice must not produce duplicate FTS rows");
}

// ── block_tags / list_block_tags / get_blocks_needing_tags ───────────────────

#[test]
fn test_list_block_tags_empty_when_none_stored() {
    let db = WeavebackDb::open_temp().unwrap();
    let r = db.list_block_tags(None).unwrap();
    assert!(r.is_empty());
}

#[test]
fn test_set_and_list_block_tags() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= Section\n\nProse here.\n";
    db.set_src_snapshot("a.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks("a.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();

    db.set_block_tags("a.adoc", 0, &[1u8; 32], "design,architecture").unwrap();
    db.set_block_tags("a.adoc", 1, &[2u8; 32], "prose,overview").unwrap();

    let r = db.list_block_tags(None).unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].block_index, 0);
    assert_eq!(r[0].tags, "design,architecture");
    assert_eq!(r[1].block_index, 1);
    assert_eq!(r[1].tags, "prose,overview");
}

#[test]
fn test_list_block_tags_file_filter() {
    let mut db = WeavebackDb::open_temp().unwrap();

    for (file, tag) in [("a.adoc", "alpha"), ("b.adoc", "beta")] {
        let src = format!("= {tag}\n\nContent.\n");
        db.set_src_snapshot(file, src.as_bytes()).unwrap();
        db.set_source_blocks(file, &[block(0, "section", 1, 1)]).unwrap();
        db.set_block_tags(file, 0, &[0u8; 32], tag).unwrap();
    }

    let r = db.list_block_tags(Some("a.adoc")).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].tags, "alpha");
}

#[test]
fn test_get_blocks_needing_tags_returns_all_when_no_tags() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= Section\n\nParagraph.\n";
    db.set_src_snapshot("a.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks("a.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();

    let need = db.get_blocks_needing_tags().unwrap();
    assert_eq!(need.len(), 2);
}

#[test]
fn test_get_blocks_needing_tags_skips_up_to_date() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= Section\n\nParagraph.\n";
    db.set_src_snapshot("a.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks("a.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();

    // Tag both with the same hash as stored in source_blocks ([0u8; 32]).
    db.set_block_tags("a.adoc", 0, &[0u8; 32], "design").unwrap();
    db.set_block_tags("a.adoc", 1, &[0u8; 32], "prose").unwrap();

    let need = db.get_blocks_needing_tags().unwrap();
    assert!(need.is_empty(), "no blocks should need re-tagging");
}

#[test]
fn test_get_blocks_needing_tags_returns_stale_hash() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= Section\n\nParagraph.\n";
    db.set_src_snapshot("a.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks("a.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();

    db.set_block_tags("a.adoc", 0, &[9u8; 32], "stale").unwrap(); // hash mismatch
    db.set_block_tags("a.adoc", 1, &[0u8; 32], "fresh").unwrap(); // matches

    let need = db.get_blocks_needing_tags().unwrap();
    assert_eq!(need.len(), 1);
    assert_eq!(need[0].block_index, 0);
}

#[test]
fn test_get_blocks_needing_tags_excludes_code_blocks() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= Section\n\nProse.\n\n----\ncode\n----\n";
    db.set_src_snapshot("a.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks("a.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
        block(2, "code",    5, 7),
    ]).unwrap();

    let need = db.get_blocks_needing_tags().unwrap();
    assert_eq!(need.len(), 2);
    assert!(need.iter().all(|b| b.block_type != "code"));
}

#[test]
fn test_search_result_includes_tags() {
    let mut db = WeavebackDb::open_temp().unwrap();

    let source = "= Tagging\n\nThis block has tags.\n";
    db.set_src_snapshot("a.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks("a.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();
    db.set_block_tags("a.adoc", 1, &[0u8; 32], "testing,fts").unwrap();

    db.rebuild_prose_fts().unwrap();

    let r = db.search_prose("tags", 10).unwrap();
    assert!(!r.is_empty());
    let para = r.iter().find(|x| x.block_type == "para").unwrap();
    assert_eq!(para.tags, "testing,fts");
}

#[test]
fn test_search_finds_block_by_tag_word() {
    let mut db = WeavebackDb::open_temp().unwrap();

    // Prose does NOT contain the word "incremental" — it's only in the tag.
    let source = "= Build System\n\nThis section describes how building works.\n";
    db.set_src_snapshot("a.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks("a.adoc", &[
        block(0, "section", 1, 1),
        block(1, "para",    3, 3),
    ]).unwrap();
    db.set_block_tags("a.adoc", 1, &[0u8; 32], "incremental,build").unwrap();

    db.rebuild_prose_fts().unwrap();

    let r = db.search_prose("incremental", 10).unwrap();
    assert!(!r.is_empty(), "tag-only word should be findable via FTS");
    assert_eq!(r[0].src_file, "a.adoc");
}

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
