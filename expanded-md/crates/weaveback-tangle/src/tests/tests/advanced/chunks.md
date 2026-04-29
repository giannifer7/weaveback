# Chunk Parsing and Core Errors





```rust
// <[@file weaveback-tangle/src/tests/advanced/chunks.rs]>=
// weaveback-tangle/src/tests/advanced/chunks.rs
// I'd Really Rather You Didn't edit this generated file.

use super::super::*;
use crate::{ChunkError, WeavebackError};

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

// @@
```

