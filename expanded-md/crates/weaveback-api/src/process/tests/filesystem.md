# Single-Pass Filesystem Tests

```rust
// <[@file weaveback-api/src/process/tests/filesystem.rs]>=
// weaveback-api/src/process/tests/filesystem.rs
// I'd Really Rather You Didn't edit this generated file.

use super::super::{find_files, write_depfile};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

// <[process-test-filesystem]>

// @
```


```rust
// <[process-test-filesystem]>=
#[test]
fn find_files_discovers_matching_extensions() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("a.adoc"), b"").unwrap();
    fs::write(tmp.path().join("b.txt"), b"").unwrap();
    let sub = tmp.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("c.adoc"), b"").unwrap();

    let mut out = Vec::new();
    find_files(tmp.path(), &["adoc".to_string()], &mut out).unwrap();
    out.sort();
    assert_eq!(out.len(), 2);
    assert!(out[0].ends_with("a.adoc") || out[1].ends_with("a.adoc"));
    assert!(out.iter().any(|p| p.ends_with("c.adoc")));
    assert!(!out.iter().any(|p| p.ends_with("b.txt")));
}
#[test]
fn find_files_returns_empty_for_no_match() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("x.txt"), b"").unwrap();
    let mut out = Vec::new();
    find_files(tmp.path(), &["adoc".to_string()], &mut out).unwrap();
    assert!(out.is_empty());
}
#[test]
fn write_depfile_produces_makefile_format() {
    let tmp = tempdir().unwrap();
    let dep_path = tmp.path().join("out.d");
    let target = std::path::Path::new("out.rs");
    let deps = vec![
        PathBuf::from("src/a.adoc"),
        PathBuf::from("src/b.adoc"),
    ];
    write_depfile(&dep_path, target, &deps).unwrap();
    let content = fs::read_to_string(&dep_path).unwrap();
    assert!(content.starts_with("out.rs:"));
    assert!(content.contains("src/a.adoc"));
    assert!(content.contains("src/b.adoc"));
    assert!(content.ends_with('\n'));
}
#[test]
fn write_depfile_escapes_spaces_in_paths() {
    let tmp = tempdir().unwrap();
    let dep_path = tmp.path().join("out.d");
    let target = std::path::Path::new("my out.rs");
    let deps = vec![PathBuf::from("src/my file.adoc")];
    write_depfile(&dep_path, target, &deps).unwrap();
    let content = fs::read_to_string(&dep_path).unwrap();
    assert!(content.contains(r"my\ out.rs"));
    assert!(content.contains(r"my\ file.adoc"));
}
#[test]
fn find_files_error_on_missing_dir() {
    let res = find_files(std::path::Path::new("/non/existent/path/for/weaveback/test"), &["adoc".to_string()], &mut Vec::new());
    assert!(res.is_err());
}
// @
```

