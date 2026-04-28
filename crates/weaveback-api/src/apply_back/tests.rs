// weaveback-api/src/apply_back/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use std::fs;
use weaveback_tangle::db::{ChunkDefEntry, Confidence};

fn lines(s: &str) -> Vec<String> {
    s.lines().map(str::to_string).collect()
}

struct TestWorkspace {
    root: std::path::PathBuf,
}
impl TestWorkspace {
    fn new() -> Self {
        let unique = format!(
            "wb-apply-back-tests-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }
    fn write_file(&self, rel: &str, content: &[u8]) {
        let path = self.root.join(rel);
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }
    fn open_db(&self) -> WeavebackDb {
        WeavebackDb::open(self.root.join("weaveback.db")).unwrap()
    }
}
impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}


mod apply_file;
mod batch;
mod primitives;
mod resolution;
mod runner;
mod workspace;

