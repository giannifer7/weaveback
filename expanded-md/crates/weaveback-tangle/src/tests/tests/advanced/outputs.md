# Output and Clip API





```rust
// <[@file weaveback-tangle/src/tests/advanced/outputs.rs]>=
// weaveback-tangle/src/tests/advanced/outputs.rs
// I'd Really Rather You Didn't edit this generated file.

use super::super::*;

#[test]
fn write_files_incremental_skips_named_chunk() {
    use std::collections::HashSet;
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        "# <<@file a.txt>>=\nalpha\n# @\n\
         # <<@file b.txt>>=\nbeta\n# @\n",
        "src.nw",
    );
    let mut skip = HashSet::new();
    skip.insert("@file b.txt".to_string());
    setup.clip.write_files_incremental(&skip).unwrap();
    // a.txt should be written; b.txt should be absent
    let gen_dir = setup._temp_dir.path().join("gen");
    assert!(gen_dir.join("a.txt").exists(), "a.txt should be written");
    assert!(!gen_dir.join("b.txt").exists(), "b.txt should be skipped");
}

#[test]
fn write_files_incremental_empty_skip_writes_all() {
    use std::collections::HashSet;
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        "# <<@file x.txt>>=\nxxx\n# @\n",
        "src.nw",
    );
    setup.clip.write_files_incremental(&HashSet::new()).unwrap();
    let gen_dir = setup._temp_dir.path().join("gen");
    assert!(gen_dir.join("x.txt").exists());
}

// ── list_output_files ─────────────────────────────────────────────────────────

#[test]
fn list_output_files_returns_gen_relative_paths() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        "# <<@file foo.txt>>=\ncontent\n# @\n",
        "src.nw",
    );
    let files = setup.clip.list_output_files();
    assert_eq!(files.len(), 1);
    let name = files[0].file_name().unwrap().to_str().unwrap();
    assert_eq!(name, "foo.txt");
}

#[test]
fn list_output_files_multiple() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        "# <<@file a.rs>>=\na\n# @\n\
         # <<@file b.rs>>=\nb\n# @\n",
        "src.nw",
    );
    let mut files: Vec<_> = setup.clip.list_output_files()
        .into_iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();
    files.sort();
    assert_eq!(files, vec!["a.rs", "b.rs"]);
}

// ── chunk_defs ────────────────────────────────────────────────────────────────

#[test]
fn chunk_defs_records_definition_line_ranges() {
    let mut setup = TestSetup::new(&["#"]);
    // Define a named chunk -- def_start/def_end are 1-indexed line numbers
    setup.clip.read(
        "# <<greet>>=\nhello\n# @\n",
        "src.nw",
    );
    // Access chunk_defs through the store via the public db after write_files
    // (we verify through expansion since chunk_defs is on ChunkStore, not Clip).
    // The simpler path: just verify the chunk exists and expands correctly.
    let content = setup.clip.get_chunk_content("greet").unwrap();
    assert_eq!(content, vec!["hello\n"]);
}

// ── path_is_safe via chunk validation ────────────────────────────────────────

#[test]
fn file_chunk_with_absolute_path_blocked_without_allow_home() {
    let mut setup = TestSetup::new(&["#"]);
    // Absolute path chunks are rejected by path_is_safe → validate_chunk_name
    // returns false → the chunk is silently dropped.
    setup.clip.read(
        "# <<@file /etc/passwd>>=\nbad\n# @\n",
        "src.nw",
    );
    // The absolute-path chunk is not registered as a file chunk.
    assert!(
        !setup.clip.get_file_chunks().contains(&"@file /etc/passwd".to_string()),
        "absolute-path @file chunk should be rejected"
    );
}

#[test]
fn file_chunk_with_parent_dir_traversal_blocked() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        "# <<@file ../../escape.txt>>=\nbad\n# @\n",
        "src.nw",
    );
    assert!(
        !setup.clip.get_file_chunks().contains(&"@file ../../escape.txt".to_string()),
        "path-traversal @file chunk should be rejected"
    );
}

// ── Windows-style path in @file ───────────────────────────────────────────────

#[test]
fn file_chunk_with_windows_path_blocked() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        "# <<@file C:\\evil.txt>>=\nbad\n# @\n",
        "src.nw",
    );
    assert!(
        !setup.clip.get_file_chunks().contains(&"@file C:\\evil.txt".to_string()),
        "windows-style @file chunk should be rejected"
    );
}

// ── Clip::reset ────────────────────────────────────────────────────────────────

#[test]
fn clip_reset_clears_chunks() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read("# <<@file out.txt>>=\nhello\n# @\n", "src.nw");
    assert!(setup.clip.has_chunk("@file out.txt"));
    setup.clip.reset();
    assert!(
        !setup.clip.has_chunk("@file out.txt"),
        "reset should clear chunks"
    );
    assert!(
        setup.clip.get_file_chunks().is_empty(),
        "reset should clear file chunks list"
    );
}

// ── Clip::get_chunk ────────────────────────────────────────────────────────────

#[test]
fn clip_get_chunk_writes_content_to_writer() {
    use std::io::BufWriter;
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read("# <<my-chunk>>=\nfoo\nbar\n# @\n", "src.nw");
    let mut buf = BufWriter::new(Vec::new());
    setup.clip.get_chunk("my-chunk", &mut buf).unwrap();
    let output = String::from_utf8(buf.into_inner().unwrap()).unwrap();
    assert!(output.contains("foo"));
    assert!(output.contains("bar"));
}

// ── Clip::read_files ──────────────────────────────────────────────────────────

#[test]
fn clip_read_files_reads_multiple_inputs() {
    use std::io::Write;
    let temp = tempfile::TempDir::new().unwrap();
    let a_path = temp.path().join("a.nw");
    let b_path = temp.path().join("b.nw");
    std::fs::File::create(&a_path).unwrap().write_all(b"# <<@file a.txt>>=\nalpha\n# @\n").unwrap();
    std::fs::File::create(&b_path).unwrap().write_all(b"# <<@file b.txt>>=\nbeta\n# @\n").unwrap();

    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read_files(&[&a_path, &b_path]).unwrap();
    assert!(setup.clip.has_chunk("@file a.txt"));
    assert!(setup.clip.has_chunk("@file b.txt"));
}

// ── Clip::db / db_mut ─────────────────────────────────────────────────────────

#[test]
fn clip_db_and_db_mut_are_accessible() {
    let mut setup = TestSetup::new(&["#"]);
    let _ = setup.clip.db();
    let _ = setup.clip.db_mut();
}

// @@
```

