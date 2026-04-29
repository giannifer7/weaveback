# Noweb Map Queries





```rust
// <[@file weaveback-tangle/src/tests/fts/noweb_map.rs]>=
// weaveback-tangle/src/tests/fts/noweb_map.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

#[test]
fn db_get_noweb_entry_roundtrips() {
    use crate::db::{Confidence, NowebMapEntry};
    let mut db = WeavebackDb::open_temp().unwrap();
    let entry = NowebMapEntry {
        src_file: "src/foo.adoc".to_string(),
        chunk_name: "greet".to_string(),
        src_line: 10,
        indent: "  ".to_string(),
        confidence: Confidence::Exact,
    };
    db.set_noweb_entries("gen/out.rs", &[(5, entry.clone())]).unwrap();
    let result = db.get_noweb_entry("gen/out.rs", 5).unwrap();
    let got = result.expect("entry should exist");
    assert_eq!(got.src_file, "src/foo.adoc");
    assert_eq!(got.chunk_name, "greet");
    assert_eq!(got.src_line, 10);
}

#[test]
fn db_get_noweb_entry_missing_returns_none() {
    let db = WeavebackDb::open_temp().unwrap();
    assert!(db.get_noweb_entry("gen/out.rs", 99).unwrap().is_none());
}

#[test]
fn db_get_noweb_entry_by_suffix() {
    use crate::db::{Confidence, NowebMapEntry};
    let mut db = WeavebackDb::open_temp().unwrap();
    let entry = NowebMapEntry {
        src_file: "src/foo.adoc".to_string(),
        chunk_name: "hello".to_string(),
        src_line: 3,
        indent: String::new(),
        confidence: Confidence::Exact,
    };
    db.set_noweb_entries("crates/foo/src/lib.rs", &[(0, entry)]).unwrap();
    let got = db.get_noweb_entry_by_suffix("foo/src/lib.rs", 0).unwrap();
    assert!(got.is_some(), "should find via suffix");
    assert_eq!(got.unwrap().chunk_name, "hello");
}

#[test]
fn db_get_noweb_entries_for_file_by_suffix() {
    use crate::db::{Confidence, NowebMapEntry};
    let mut db = WeavebackDb::open_temp().unwrap();
    let entries: Vec<(u32, NowebMapEntry)> = (0..3).map(|i| (i, NowebMapEntry {
        src_file: "src.adoc".to_string(),
        chunk_name: format!("c{}", i),
        src_line: i + 1,
        indent: String::new(),
        confidence: Confidence::Exact,
    })).collect();
    db.set_noweb_entries("gen/out.rs", &entries).unwrap();
    let result = db.get_noweb_entries_for_file_by_suffix("out.rs").unwrap();
    assert_eq!(result.len(), 3);
}

#[test]
fn db_query_chunk_output_files() {
    use crate::db::{Confidence, NowebMapEntry};
    let mut db = WeavebackDb::open_temp().unwrap();
    let entry = NowebMapEntry {
        src_file: "src.adoc".to_string(),
        chunk_name: "main-chunk".to_string(),
        src_line: 1,
        indent: String::new(),
        confidence: Confidence::Exact,
    };
    db.set_noweb_entries("gen/out.rs", &[(0, entry)]).unwrap();
    let files = db.query_chunk_output_files("main-chunk").unwrap();
    assert_eq!(files, vec!["gen/out.rs".to_string()]);
}

#[test]
fn db_get_output_location_roundtrips() {
    use crate::db::{Confidence, NowebMapEntry};
    let mut db = WeavebackDb::open_temp().unwrap();
    let entry = NowebMapEntry {
        src_file: "src.adoc".to_string(),
        chunk_name: "chunk".to_string(),
        src_line: 42,
        indent: String::new(),
        confidence: Confidence::Exact,
    };
    db.set_noweb_entries("gen/out.rs", &[(7, entry)]).unwrap();
    let loc = db.get_output_location("src.adoc", 42).unwrap();
    assert!(loc.is_some());
    let (file, line) = loc.unwrap();
    assert_eq!(file, "gen/out.rs");
    assert_eq!(line, 7);
}

#[test]
fn db_get_all_output_mappings() {
    use crate::db::{Confidence, NowebMapEntry};
    let mut db = WeavebackDb::open_temp().unwrap();
    let entries: Vec<(u32, NowebMapEntry)> = (0..2).map(|i| (i, NowebMapEntry {
        src_file: "s.adoc".to_string(),
        chunk_name: "c".to_string(),
        src_line: i + 1,
        indent: String::new(),
        confidence: Confidence::Exact,
    })).collect();
    db.set_noweb_entries("gen/x.rs", &entries).unwrap();
    let mappings = db.get_all_output_mappings("s.adoc").unwrap();
    assert_eq!(mappings.len(), 2);
}

// @@
```

