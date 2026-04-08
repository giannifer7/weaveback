// crates/weaveback-macro/src/evaluator/tests/test_source_utils.rs
use std::fs;

use tempfile::TempDir;

use crate::evaluator::source_utils::modify_source;

#[test]
fn test_modify_source_applies_sorted_insertions() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("src.txt");
    fs::write(&path, "abcd").unwrap();

    modify_source(
        &path,
        &[
            (3, b"Y".to_vec(), false),
            (1, b"X".to_vec(), false),
        ],
    )
    .unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "aXbcYd");
}

#[test]
fn test_modify_source_can_skip_replaced_line_tail() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("src.txt");
    fs::write(&path, "head\nreplace me\nkeep\n").unwrap();

    modify_source(&path, &[(5, b"NEW\n".to_vec(), true)]).unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "head\nNEW\nkeep\n");
}

#[test]
fn test_modify_source_insert_beyond_eof_appends() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("src.txt");
    fs::write(&path, "abc").unwrap();

    modify_source(&path, &[(99, b"XYZ".to_vec(), false)]).unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "abcXYZ");
}
