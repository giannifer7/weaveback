// src/tests/basic.rs
use super::*;
use crate::ChunkError;

#[test]
fn test_basic_chunk() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(BASIC_CHUNK, "test_basic.nw");

    assert!(setup.clip.has_chunk("test"));
    assert_eq!(
        setup.clip.get_chunk_content("test").unwrap(),
        vec!["Hello\n"]
    );
}

#[test]
fn test_multiple_chunks() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(TWO_CHUNKS, "test_multiple.nw");

    assert!(setup.clip.has_chunk("chunk1"));
    assert!(setup.clip.has_chunk("chunk2"));
    assert_eq!(
        setup.clip.get_chunk_content("chunk1").unwrap(),
        vec!["First chunk\n"]
    );
    assert_eq!(
        setup.clip.get_chunk_content("chunk2").unwrap(),
        vec!["Second chunk\n"]
    );
}

#[test]
fn test_nested_chunk_expansion() -> Result<(), ChunkError> {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(NESTED_CHUNKS, "test_nested.nw");

    let expanded = setup.clip.expand("outer", "")?;
    let expected = vec!["Before\n", "Nested content\n", "After\n"];
    assert_eq!(expanded, expected, "Nested chunks should expand correctly");
    Ok(())
}

#[test]
fn test_indentation_preservation() -> Result<(), ChunkError> {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(INDENTED_CHUNK, "test_indent.nw");

    let expanded = setup.clip.expand("main", "")?;
    assert_eq!(
        expanded,
        vec!["    some code\n"],
        "Indentation should be preserved"
    );
    Ok(())
}

#[test]
fn test_complex_indentation() -> Result<(), ChunkError> {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(PYTHON_CODE, "test_python.nw");

    let expanded = setup.clip.expand("code", "")?;
    let expected = vec!["def example():\n", "    print('hello')\n"];
    assert_eq!(expanded, expected);

    let expanded_indented = setup.clip.expand("code", "  ")?;
    let expected_indented = vec!["  def example():\n", "      print('hello')\n"];
    assert_eq!(expanded_indented, expected_indented);
    Ok(())
}

#[test]
fn test_multi_comment_styles() {
    let mut setup = TestSetup::new(&["#", "//"]);
    setup.clip.read(MULTI_COMMENT_CHUNKS, "test_comments.nw");

    assert!(setup.clip.has_chunk("python_chunk"));
    assert!(setup.clip.has_chunk("rust_chunk"));

    let python_content = setup.clip.get_chunk_content("python_chunk").unwrap();
    assert!(python_content.join("").contains("print(\"Hello\")"));

    let rust_content = setup.clip.get_chunk_content("rust_chunk").unwrap();
    assert!(rust_content.join("").contains("println!(\"Hello\")"));
}

#[test]
fn test_sequential_chunks() -> Result<(), ChunkError> {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(SEQUENTIAL_CHUNKS, "test_sequential.nw");

    let expanded = setup.clip.expand("main", "")?;
    assert_eq!(expanded, vec!["First part\n", "Second part\n"]);
    Ok(())
}

#[test]
fn test_empty_chunk() {
    let mut setup = TestSetup::new(&["#"]);
    setup.clip.read(EMPTY_CHUNK, "test_empty.nw");

    assert!(setup.clip.has_chunk("empty"));
    assert!(
        setup.clip.get_chunk_content("empty").unwrap().is_empty(),
        "empty chunk should have no content"
    );
}
