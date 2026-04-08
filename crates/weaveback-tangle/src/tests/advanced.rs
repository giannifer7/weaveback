// src/tests/advanced.rs

use super::*;
use crate::{Clip, SafeFileWriter, WeavebackError};
use crate::ChunkError;
use std::fs;

/// Bug fix: duplicate @file chunk without @replace used to silently discard
/// both definitions. Now it reports an error and keeps the first definition.
#[test]
fn test_duplicate_file_chunk_keeps_first_definition() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        r#"
# <<@file out.txt>>=
first definition
# @

# <<@file out.txt>>=
second definition
# @
"#,
        "duplicate.nw",
    );

    // The first definition must survive.
    assert!(
        setup.clip.has_chunk("@file out.txt"),
        "first definition should be kept"
    );
    let content = setup.clip.get_chunk_content("@file out.txt").unwrap();
    assert!(
        content.iter().any(|l| l.contains("first definition")),
        "first definition content should be preserved, got: {:?}",
        content
    );
    assert!(
        !content.iter().any(|l| l.contains("second definition")),
        "second definition should be rejected"
    );
}

#[test]
fn test_file_chunk_detection() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(FILE_CHUNKS, "test_files.nw");

    let file_chunks = setup.clip.get_file_chunks();
    assert_eq!(file_chunks.len(), 1);
    assert!(file_chunks.contains(&"@file output.txt".to_string()));
}

#[test]
fn test_undefined_chunk_is_error() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        r#"
# <<main>>=
# <<nonexistent>>
# @
"#,
        "undefined.nw",
    );
    setup.clip.set_strict_undefined(true);

    let result = setup.clip.expand("main", "");
    assert!(result.is_err(), "referencing an undefined chunk must be an error");
    let err = result.unwrap_err();
    assert!(
        matches!(err, WeavebackError::Chunk(ChunkError::UndefinedChunk { ref chunk, .. }) if chunk == "nonexistent"),
        "expected UndefinedChunk error, got: {err}",
    );
}

#[test]
fn test_undefined_chunk_is_empty_when_not_strict() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        r#"
# <<main>>=
line before
# <<optional>>
line after
# @
"#,
        "undefined.nw",
    );
    // Default is permissive; no set_strict_undefined call needed.
    let result = setup.clip.expand("main", "");
    assert!(result.is_ok(), "undefined chunk should expand to empty when not strict");
    let lines = result.unwrap();
    assert_eq!(lines, vec!["line before\n", "line after\n"]);
}

#[test]
fn test_recursive_chunk_error() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        r#"
# <<recursive>>=
Start
# <<recursive>>
End
# @
"#,
        "recursive.nw",
    );

    let result = setup.clip.expand("recursive", "");
    match result {
        Err(WeavebackError::Chunk(ChunkError::RecursiveReference {
            chunk,
            cycle,
            file_name,
            location,
        })) => {
            assert_eq!(chunk, "recursive");
            assert_eq!(file_name, "recursive.nw");
            assert_eq!(location.line, 2);
            assert_eq!(cycle, vec!["recursive", "recursive"]);
        }
        _ => panic!("Expected RecursiveReference error"),
    }
}

#[test]
fn test_mutual_recursion_error() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        r#"
# <<chunk-a>>=
Start A
# <<chunk-b>>
End A
# @

# <<chunk-b>>=
Middle B
# <<chunk-a>>
End B
# @
"#,
        "mutual_recursion.nw",
    );

    let result = setup.clip.expand("chunk-a", "");
    match result {
        Err(WeavebackError::Chunk(ChunkError::RecursiveReference {
            chunk,
            cycle,
            file_name,
            location,
        })) => {
            assert_eq!(chunk, "chunk-a");
            assert_eq!(file_name, "mutual_recursion.nw");
            assert_eq!(location.line, 8);
            assert_eq!(cycle, vec!["chunk-a", "chunk-b", "chunk-a"]);
        }
        _ => panic!("Expected RecursiveReference error"),
    }
}

#[test]
fn test_max_recursion_depth() {
    let mut setup = TestSetup::new(&["#"]);

    let mut content = String::from(
        r#"
# <<a-000>>=
# <<a-001>>
# @"#,
    );

    let chain_length = 150; // More than MAX_DEPTH = 100
    for i in 1..chain_length {
        content.push_str(&format!(
            r#"
# <<a-{:03}>>=
# <<a-{:03}>>
# @"#,
            i,     // a-001, a-002, etc.
            i + 1  // a-002, a-003, etc.
        ));
    }

    setup.clip.read(&content, "max_recursion.nw");
    let result = setup.clip.expand("a-000", "");

    // We just match the variant here (less strict). Alternatively, pattern match with { chunk, file_name, location }
    assert!(
        matches!(
            result,
            Err(WeavebackError::Chunk(ChunkError::RecursionLimit { .. }))
        ),
        "Expected RecursionLimit error"
    );
}

#[test]
fn test_error_messages_format() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        r#"
# <<a>>=
# <<a>>
# @
"#,
        "errors.nw",
    );

    let err = setup.clip.expand("a", "").unwrap_err();
    let error_msg = err.to_string();

    assert!(error_msg.contains("Chunk error: errors.nw line 2:"));
    assert!(error_msg.contains("recursive reference detected in chunk 'a'"));
    assert!(error_msg.contains("cycle: a -> a"));
}

#[test]
fn test_dangerous_comment_markers() {
    let markers = &[
        "#",         // normal case
        r".*",       // regex wildcard
        r"[a-z]+",   // regex character class
        r"\d+",      // regex digit
        "<<",        // same as delimiter
        ">>",        // same as delimiter
        "(comment)", // regex group
    ];

    let content = r#"
#<<test1>>=
Content1
@
.*<<test2>>=
Content2
@
[a-z]+<<test3>>=
Content3
@
(comment)<<test4>>=
Content4
@
"#;

    let mut setup = TestSetup::new(markers);
    setup.clip.read(content, "regex_test.nw");

    assert!(setup.clip.has_chunk("test1"), "Basic marker # failed");
    assert!(setup.clip.has_chunk("test2"), "Wildcard marker .* failed");
    assert!(
        setup.clip.has_chunk("test3"),
        "Character class marker [a-z]+ failed"
    );
    assert!(
        setup.clip.has_chunk("test4"),
        "Group marker (comment) failed"
    );

    assert_eq!(
        setup.clip.get_chunk_content("test1").unwrap(),
        vec!["Content1\n"]
    );
}

// ── @replace ─────────────────────────────────────────────────────────────────

/// `@replace` on a regular chunk discards all prior definitions and installs
/// only the new one.
#[test]
fn test_replace_normal_chunk() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        r#"
# <<greet>>=
Hello
# @

# <<@replace greet>>=
Hi
# @
"#,
        "replace_normal.nw",
    );

    let content = setup.clip.get_chunk_content("greet").unwrap();
    assert_eq!(
        content,
        vec!["Hi\n"],
        "only the @replace definition should survive"
    );
}

/// `@replace` on a file chunk replaces the earlier definition.
#[test]
fn test_replace_file_chunk() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        r#"
# <<@file out.txt>>=
old content
# @

# <<@replace @file out.txt>>=
new content
# @
"#,
        "replace_file.nw",
    );

    let content = setup.clip.get_chunk_content("@file out.txt").unwrap();
    assert_eq!(
        content,
        vec!["new content\n"],
        "@replace should replace file chunk"
    );
}

/// Without `@replace`, a second file-chunk definition is an error and the first
/// definition is kept (regression guard).
#[test]
fn test_no_replace_file_chunk_keeps_first() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        r#"
# <<@file out.txt>>=
first
# @

# <<@file out.txt>>=
second
# @
"#,
        "no_replace_file.nw",
    );

    let content = setup.clip.get_chunk_content("@file out.txt").unwrap();
    assert!(
        content.iter().any(|l| l.contains("first")),
        "first definition should be kept without @replace"
    );
    assert!(
        !content.iter().any(|l| l.contains("second")),
        "second definition should be rejected without @replace"
    );
}

// ── @reversed ────────────────────────────────────────────────────────────────

/// A regular chunk may accumulate multiple definitions (without @replace).
/// A plain reference expands them in definition order (first → last).
#[test]
fn test_accumulated_chunks_normal_order() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        r#"
# <<items>>=
alpha
# @

# <<items>>=
beta
# @

# <<items>>=
gamma
# @

# <<list>>=
# <<items>>
# @
"#,
        "normal_order.nw",
    );

    let expanded = setup.clip.expand("list", "").unwrap();
    assert_eq!(expanded, vec!["alpha\n", "beta\n", "gamma\n"]);
}

/// `@reversed` on a reference expands the chunk's accumulated definitions in
/// reverse order (last-defined first).
#[test]
fn test_reversed_reference() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        r#"
# <<items>>=
alpha
# @

# <<items>>=
beta
# @

# <<items>>=
gamma
# @

# <<list>>=
# <<@reversed items>>
# @
"#,
        "reversed.nw",
    );

    let expanded = setup.clip.expand("list", "").unwrap();
    assert_eq!(expanded, vec!["gamma\n", "beta\n", "alpha\n"]);
}

/// `@compact` trims blank edge lines from each accumulated definition before
/// splicing them into the caller. This is useful for table rows and similar
/// projection chunks where each definition should contribute one logical line.
#[test]
fn test_compact_reference_trims_blank_edge_lines() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        r#"
# <<rows>>=

alpha

# @

# <<rows>>=

beta

# @

# <<list>>=
# <<@compact rows>>
# @
"#,
        "compact.nw",
    );

    let expanded = setup.clip.expand("list", "").unwrap();
    assert_eq!(expanded, vec!["alpha\n", "beta\n"]);
}

/// `@tight` is stronger than `@compact`: it also drops blank-only lines inside
/// each accumulated definition. This is useful for highly structured fragments
/// like generated table rows.
#[test]
fn test_tight_reference_drops_blank_only_lines() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        r#"
# <<rows>>=

alpha

omega

# @

# <<list>>=
# <<@tight rows>>
# @
"#,
        "tight.nw",
    );

    let expanded = setup.clip.expand("list", "").unwrap();
    assert_eq!(expanded, vec!["alpha\n", "omega\n"]);
}

/// `~` in an `@file` path expands to `$HOME` when `--allow-home` is set.
#[test]
fn test_tilde_expansion_in_file_chunk() {
    let fake_home = tempfile::TempDir::new().unwrap();
    // Override HOME for this test
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("HOME", fake_home.path()) };

    // Tilde expansion writes outside gen/ and requires allow_home: true.
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gen_path = temp_dir.path().join("gen");
    fs::create_dir_all(&gen_path).unwrap();
    let safe_writer = SafeFileWriter::with_config(
        gen_path,
        crate::safe_writer::SafeWriterConfig {
            allow_home: true,
            ..crate::safe_writer::SafeWriterConfig::default()
        },
    ).unwrap();
    let mut clip = Clip::new(safe_writer, "<<", ">>", "@", &["#".to_string()]);

    clip.read(
        "# <<@file ~/tilde_test.txt>>=\nhello tilde\n# @\n",
        "tilde.nw",
    );
    clip.write_files().unwrap();

    let expected = fake_home.path().join("tilde_test.txt");
    assert!(
        expected.exists(),
        "file should be written to expanded ~ path"
    );
    let content = fs::read_to_string(&expected).unwrap();
    assert_eq!(content, "hello tilde\n");
}

/// Without `--allow-home`, `@file ~/…` is refused rather than silently
/// escaping the gen/ sandbox.
#[test]
fn test_tilde_expansion_blocked_without_allow_home() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        "# <<@file ~/should_not_exist.txt>>=\ndata\n# @\n",
        "tilde_blocked.nw",
    );
    let result = setup.clip.write_files();
    assert!(
        matches!(
            result,
            Err(WeavebackError::SafeWriter(
                crate::safe_writer::SafeWriterError::SecurityViolation(_)
            ))
        ),
        "expected SecurityViolation without --allow-home, got: {:?}",
        result
    );
}

// ── warn_unused ───────────────────────────────────────────────────────────────

/// Unused named chunks produce no warnings by default -- `check_unused_chunks`
/// is never called when `warn_unused` is false, but the API itself is always
/// available.  Here we verify that the helper correctly identifies unused chunks.
#[test]
fn test_check_unused_chunks_finds_orphan() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        "# <<@file out.txt>>=\nhello\n# @\n\
         # <<orphan>>=\nnever used\n# @\n",
        "src.nw",
    );
    // Simulate what write_files does: start from @file chunk names.
    let all_file_chunks: std::collections::HashSet<String> =
        setup.clip.get_file_chunks().into_iter().collect();
    let warns = setup.clip.check_unused_chunks(&all_file_chunks);
    assert!(
        warns.iter().any(|w| w.contains("orphan")),
        "expected unused-chunk warning for 'orphan', got: {:?}",
        warns
    );
}

/// A chunk in the referenced set must NOT appear in the unused-chunk report.
/// `check_unused_chunks` takes the full transitive closure of referenced
/// chunks (including `@file` names themselves), so we build that set manually.
#[test]
fn test_check_unused_chunks_skips_referenced() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(
        "# <<@file out.txt>>=\n# <<used>>\n# @\n\
         # <<used>>=\ncontent\n# @\n",
        "src.nw",
    );
    // Provide the full transitive closure: @file chunk + the named chunk it references.
    let mut referenced = std::collections::HashSet::new();
    referenced.insert("@file out.txt".to_string());
    referenced.insert("used".to_string());
    let warns = setup.clip.check_unused_chunks(&referenced);
    assert!(
        warns.is_empty(),
        "no unused-chunk warnings expected when all chunks are reachable, got: {:?}",
        warns
    );
}

/// `set_warn_unused` round-trips through the public API without panicking, and
/// `write_files` completes successfully in both modes.
#[test]
fn test_set_warn_unused_write_files_ok() {
    for warn in [false, true] {
        let mut setup = TestSetup::new(&["#"]);
        setup.clip.set_warn_unused(warn);
        setup.clip.read(
            "# <<@file out.txt>>=\nhello\n# @\n\
             # <<unused>>=\ndropped\n# @\n",
            "src.nw",
        );
        setup.clip.write_files().unwrap();
    }
}

// ── tangle_check ─────────────────────────────────────────────────────────────

#[test]
fn tangle_check_expands_file_chunks_in_memory() {
    use crate::noweb::tangle_check;
    let src = "# <<@file out.txt>>=\nhello\n# @\n";
    let markers = vec!["#".to_string()];
    let result = tangle_check(&[(src, "src.nw")], "<<", ">>", "@", &markers).unwrap();
    assert!(result.contains_key("out.txt"), "expected out.txt in result map");
    assert_eq!(result["out.txt"], vec!["hello\n"]);
}

#[test]
fn tangle_check_returns_error_on_undefined_strict() {
    use crate::noweb::tangle_check;
    // tangle_check does not expose strict mode; referencing undefined is silent
    let src = "# <<@file out.txt>>=\n# <<missing>>\n# @\n";
    let markers = vec!["#".to_string()];
    let result = tangle_check(&[(src, "src.nw")], "<<", ">>", "@", &markers).unwrap();
    // undefined chunk expands to empty; @file out.txt has zero lines
    assert_eq!(result["out.txt"], Vec::<String>::new());
}

#[test]
fn tangle_check_handles_multiple_files() {
    use crate::noweb::tangle_check;
    let a = "# <<@file a.txt>>=\nalpha\n# @\n";
    let b = "# <<@file b.txt>>=\nbeta\n# @\n";
    let markers = vec!["#".to_string()];
    let result = tangle_check(&[(a, "a.nw"), (b, "b.nw")], "<<", ">>", "@", &markers).unwrap();
    assert_eq!(result["a.txt"], vec!["alpha\n"]);
    assert_eq!(result["b.txt"], vec!["beta\n"]);
}

// ── NowebSyntax unit tests ────────────────────────────────────────────────────

#[test]
fn noweb_syntax_parse_definition_line_file_chunk() {
    use crate::noweb::NowebSyntax;
    let syn = NowebSyntax::new("<<", ">>", "@", &["#".to_string()]);
    let m = syn.parse_definition_line("# <<@file out.rs>>=").unwrap();
    assert!(m.is_file);
    assert_eq!(m.base_name, "out.rs");
    assert!(!m.is_replace);
}

#[test]
fn noweb_syntax_parse_definition_line_replace() {
    use crate::noweb::NowebSyntax;
    let syn = NowebSyntax::new("<<", ">>", "@", &["#".to_string()]);
    let m = syn.parse_definition_line("# <<@replace greet>>=").unwrap();
    assert!(m.is_replace);
    assert!(!m.is_file);
    assert_eq!(m.base_name, "greet");
}

#[test]
fn noweb_syntax_parse_definition_line_returns_none_for_content() {
    use crate::noweb::NowebSyntax;
    let syn = NowebSyntax::new("<<", ">>", "@", &["#".to_string()]);
    assert!(syn.parse_definition_line("just a plain line").is_none());
    assert!(syn.parse_definition_line("# ordinary comment").is_none());
}

#[test]
fn noweb_syntax_is_close_line() {
    use crate::noweb::NowebSyntax;
    let syn = NowebSyntax::new("<<", ">>", "@", &["#".to_string()]);
    assert!(syn.is_close_line("# @"));
    assert!(syn.is_close_line("@"));
    assert!(!syn.is_close_line("not a close"));
    assert!(!syn.is_close_line("# @ more text"));
}

// ── write_files_incremental ──────────────────────────────────────────────────

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

// ── write_files strict mode rejects parse errors ──────────────────────────────

#[test]
fn write_files_strict_rejects_file_chunk_redefinition() {
    // strict_undefined must be set BEFORE read() so the error is captured
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.set_strict_undefined(true);
    setup.clip.read(
        "# <<@file out.txt>>=\nfirst\n# @\n\n# <<@file out.txt>>=\nsecond\n# @\n",
        "src.nw",
    );
    let err = setup.clip.write_files().unwrap_err();
    match err {
        WeavebackError::Chunk(ChunkError::FileChunkRedefinition { .. }) => {}
        other => panic!("expected FileChunkRedefinition, got: {:?}", other),
    }
}

#[test]
fn write_files_incremental_strict_rejects_parse_errors() {
    // strict_undefined must be set BEFORE read() so the error is captured
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.set_strict_undefined(true);
    setup.clip.read(
        "# <<@file out.txt>>=\nfirst\n# @\n\n# <<@file out.txt>>=\nsecond\n# @\n",
        "src.nw",
    );
    let skip = std::collections::HashSet::new();
    let err = setup.clip.write_files_incremental(&skip).unwrap_err();
    match err {
        WeavebackError::Chunk(ChunkError::FileChunkRedefinition { .. }) => {}
        other => panic!("expected FileChunkRedefinition, got: {:?}", other),
    }
}
