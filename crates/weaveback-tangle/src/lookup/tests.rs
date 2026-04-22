// weaveback-tangle/src/lookup/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use crate::db::Confidence;
use std::path::PathBuf;

#[test]
fn find_best_noweb_entry_can_match_by_suffix() {
    let mut db = WeavebackDb::open_temp().expect("temp db");
    db.set_noweb_entries(
        "/tmp/wb-pass-root/weaveback/src/main.rs",
        &[(
            99,
            NowebMapEntry {
                src_file: "crates/weaveback/src/weaveback.adoc".to_string(),
                chunk_name: "main command handler".to_string(),
                src_line: 123,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .expect("set noweb entries");

    let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("crates"));
    let entry = find_best_noweb_entry(
        &db,
        "crates/weaveback/src/main.rs",
        99,
        &resolver,
    )
    .expect("lookup ok")
    .expect("entry found");

    assert_eq!(entry.chunk_name, "main command handler");
    assert_eq!(entry.src_file, "crates/weaveback/src/weaveback.adoc");
}

#[test]
fn db_suffix_lookup_prefers_shortest_matching_path() {
    let mut db = WeavebackDb::open_temp().expect("temp db");
    db.set_noweb_entries(
        "/tmp/a/weaveback/src/main.rs",
        &[(
            10,
            NowebMapEntry {
                src_file: "a.adoc".to_string(),
                chunk_name: "short".to_string(),
                src_line: 1,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .expect("set a");
    db.set_noweb_entries(
        "/tmp/very/long/prefix/weaveback/src/main.rs",
        &[(
            10,
            NowebMapEntry {
                src_file: "b.adoc".to_string(),
                chunk_name: "long".to_string(),
                src_line: 2,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .expect("set b");

    let entry = db
        .get_noweb_entry_by_suffix("weaveback/src/main.rs", 10)
        .expect("lookup ok")
        .expect("entry found");

    assert_eq!(entry.chunk_name, "short");
    assert_eq!(entry.src_file, "a.adoc");
}

#[test]
fn distinctive_suffix_candidates_stop_before_ambiguous_short_forms() {
    assert_eq!(
        distinctive_suffix_candidates("crates/weaveback/src/main.rs"),
        vec!["weaveback/src/main.rs".to_string()]
    );
    assert_eq!(
        distinctive_suffix_candidates(r"C:\tmp\ws\crate\src\main.rs"),
        vec![
            "tmp/ws/crate/src/main.rs".to_string(),
            "ws/crate/src/main.rs".to_string(),
            "crate/src/main.rs".to_string(),
        ]
    );
    assert!(distinctive_suffix_candidates("src/main.rs").is_empty());
    assert!(distinctive_suffix_candidates("main.rs").is_empty());
}

#[test]
fn find_best_noweb_entry_rejects_ambiguous_two_component_suffixes() {
    let mut db = WeavebackDb::open_temp().expect("temp db");
    db.set_noweb_entries(
        "/tmp/a/src/main.rs",
        &[(
            7,
            NowebMapEntry {
                src_file: "a.adoc".to_string(),
                chunk_name: "a".to_string(),
                src_line: 1,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .expect("set a");
    db.set_noweb_entries(
        "/tmp/b/src/main.rs",
        &[(
            7,
            NowebMapEntry {
                src_file: "b.adoc".to_string(),
                chunk_name: "b".to_string(),
                src_line: 1,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .expect("set b");

    let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("crates"));
    let got = find_best_noweb_entry(&db, "src/main.rs", 7, &resolver).expect("lookup ok");
    assert!(got.is_none());
}

// ── find_line_col ──────────────────────────────────────────────────────

#[test]
fn find_line_col_start_of_file() {
    let (line, col) = find_line_col("hello\nworld", 0);
    assert_eq!((line, col), (1, 1));
}

#[test]
fn find_line_col_end_of_first_line() {
    let text = "hello\nworld";
    let (line, col) = find_line_col(text, 5); // byte offset of '\n'
    assert_eq!((line, col), (1, 6));
}

#[test]
fn find_line_col_start_of_second_line() {
    let text = "hello\nworld";
    let (line, col) = find_line_col(text, 6); // after '\n'
    assert_eq!((line, col), (2, 1));
}

#[test]
fn find_line_col_clamps_to_text_length() {
    let text = "abc";
    let (line, col) = find_line_col(text, 999);
    assert_eq!((line, col), (1, 4)); // offset clamped to len(3), col=4
}

#[test]
fn find_line_col_empty_text() {
    let (line, col) = find_line_col("", 0);
    assert_eq!((line, col), (1, 1));
}

// ── find_best_source_config ────────────────────────────────────────────

#[test]
fn find_best_source_config_exact_match() {
    use crate::db::TangleConfig;
    let db = WeavebackDb::open_temp().unwrap();
    let cfg = TangleConfig {
        sigil: '%',
        open_delim: "<<".to_string(),
        close_delim: ">>".to_string(),
        chunk_end: "@".to_string(),
        comment_markers: vec!["#".to_string()],
    };
    db.set_source_config("src/foo.adoc", &cfg).unwrap();
    let got = find_best_source_config(&db, "src/foo.adoc").unwrap();
    assert!(got.is_some());
    assert_eq!(got.unwrap().open_delim, "<<");
}

#[test]
fn find_best_source_config_strips_dot_slash() {
    use crate::db::TangleConfig;
    let db = WeavebackDb::open_temp().unwrap();
    let cfg = TangleConfig {
        sigil: '%',
        open_delim: "<[".to_string(),
        close_delim: "]>".to_string(),
        chunk_end: "@@".to_string(),
        comment_markers: vec!["//".to_string()],
    };
    db.set_source_config("bar.adoc", &cfg).unwrap();
    // Lookup with ./ prefix — try 2 strips it
    let got = find_best_source_config(&db, "./bar.adoc").unwrap();
    assert!(got.is_some());
}

#[test]
fn find_best_source_config_adds_dot_slash() {
    use crate::db::TangleConfig;
    let db = WeavebackDb::open_temp().unwrap();
    let cfg = TangleConfig {
        sigil: '%',
        open_delim: "<<".to_string(),
        close_delim: ">>".to_string(),
        chunk_end: "@".to_string(),
        comment_markers: vec!["#".to_string()],
    };
    db.set_source_config("./baz.adoc", &cfg).unwrap();
    // Lookup without ./ prefix — try 3 prepends it
    let got = find_best_source_config(&db, "baz.adoc").unwrap();
    assert!(got.is_some());
}

#[test]
fn find_best_source_config_strips_crates_prefix() {
    use crate::db::TangleConfig;
    let db = WeavebackDb::open_temp().unwrap();
    let cfg = TangleConfig {
        sigil: '%',
        open_delim: "<<".to_string(),
        close_delim: ">>".to_string(),
        chunk_end: "@".to_string(),
        comment_markers: vec!["//".to_string()],
    };
    db.set_source_config("weaveback/src/lib.adoc", &cfg).unwrap();
    // Lookup with crates/ prefix — try 4 strips it
    let got = find_best_source_config(&db, "crates/weaveback/src/lib.adoc").unwrap();
    assert!(got.is_some());
}

#[test]
fn find_best_source_config_missing_returns_none() {
    let db = WeavebackDb::open_temp().unwrap();
    let got = find_best_source_config(&db, "nonexistent.adoc").unwrap();
    assert!(got.is_none());
}

// ── find_best_noweb_entry: exact and missing ───────────────────────────

#[test]
fn find_best_noweb_entry_exact_match() {
    let mut db = WeavebackDb::open_temp().unwrap();
    let entry = NowebMapEntry {
        src_file: "s.adoc".to_string(),
        chunk_name: "my-chunk".to_string(),
        src_line: 5,
        indent: String::new(),
        confidence: Confidence::Exact,
    };
    db.set_noweb_entries("gen/out.rs", &[(3, entry)]).unwrap();
    let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("."));
    let got = find_best_noweb_entry(&db, "gen/out.rs", 3, &resolver).unwrap();
    assert!(got.is_some());
    assert_eq!(got.unwrap().chunk_name, "my-chunk");
}

#[test]
fn find_best_noweb_entry_missing_returns_none() {
    let db = WeavebackDb::open_temp().unwrap();
    let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("."));
    let got = find_best_noweb_entry(&db, "gen/out.rs", 99, &resolver).unwrap();
    assert!(got.is_none());
}

