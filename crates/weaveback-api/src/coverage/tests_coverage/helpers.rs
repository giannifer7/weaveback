// weaveback-api/src/coverage/tests_coverage/helpers.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use rusqlite;
use serde_json::json;
use tempfile::tempdir;
use weaveback_tangle::db::{Confidence, NowebMapEntry, WeavebackDb};


fn ws_write_file(root: &Path, rel: &str, content: &[u8]) {
    let p = root.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, content).unwrap();
}

