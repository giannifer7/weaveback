# Path Safety





```rust
// <[@file weaveback-tangle/src/tests/advanced/paths.rs]>=
// weaveback-tangle/src/tests/advanced/paths.rs
// I'd Really Rather You Didn't edit this generated file.

use super::super::*;
use crate::{Clip, SafeFileWriter, WeavebackError};
use std::fs;

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

// @@
```

