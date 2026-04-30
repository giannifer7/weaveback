// weaveback-api/src/coverage/tests_coverage/helpers.rs
// I'd Really Rather You Didn't edit this generated file.

use std::path::Path;


pub(super) fn ws_write_file(root: &Path, rel: &str, content: &[u8]) {
    let p = root.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, content).unwrap();
}

