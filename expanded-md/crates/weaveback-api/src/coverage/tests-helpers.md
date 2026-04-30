# Coverage Test Helpers

Shared imports and filesystem helpers for coverage tests.

```rust
// <[coverage-tests-helpers]>=
use std::path::Path;


pub(super) fn ws_write_file(root: &Path, rel: &str, content: &[u8]) {
    let p = root.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, content).unwrap();
}
// @
```

