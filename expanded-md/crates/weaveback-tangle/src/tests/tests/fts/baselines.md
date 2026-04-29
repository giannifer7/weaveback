# Baselines





```rust
// <[@file weaveback-tangle/src/tests/fts/baselines.rs]>=
// weaveback-tangle/src/tests/fts/baselines.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

#[test]
fn db_list_baselines_returns_all_stored() {
    let db = WeavebackDb::open_temp().unwrap();
    db.set_baseline("a.rs", b"content_a").unwrap();
    db.set_baseline("b.rs", b"content_b").unwrap();
    let mut baselines = db.list_baselines().unwrap();
    baselines.sort_by_key(|(p, _)| p.clone());
    assert_eq!(baselines.len(), 2);
    assert_eq!(baselines[0].0, "a.rs");
    assert_eq!(baselines[1].0, "b.rs");
}

#[test]
fn db_get_baseline_missing_returns_none() {
    let db = WeavebackDb::open_temp().unwrap();
    assert!(db.get_baseline("nonexistent.rs").unwrap().is_none());
}

// @@
```

