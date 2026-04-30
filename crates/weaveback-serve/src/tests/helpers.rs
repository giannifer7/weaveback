// weaveback-serve/src/tests/helpers.rs
// I'd Really Rather You Didn't edit this generated file.

pub(super) use crate::{
    apply_chunk_edit,
    build_chunk_context, content_type, extract_prose, heading_level, parse_query,
    dep_bodies,
    extract_chunk_body,
    find_docgen_bin,
    find_project_root,
    git_log_for_file,
    insert_note_into_source,
    json_resp,
    percent_decode, safe_path, section_range, sse_headers, tangle_oracle, title_chain,
    run_server_loop,
    AiBackend, AiChannelReader, SseReader, TangleConfig,
};
pub(super) use std::fs;
pub(super) use std::io::Read;
pub(super) use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
pub(super) use weaveback_tangle::db::{ChunkDefEntry, Confidence, NowebMapEntry, WeavebackDb};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) struct TestWorkspace {
    pub(super) root: PathBuf,
}

impl TestWorkspace {
    pub(super) fn new() -> Self {
        let unique = format!(
            "wb-serve-tests-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock drifted backwards")
                .as_nanos()
                + u128::from(TEST_COUNTER.fetch_add(1, Ordering::Relaxed))
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(&root).expect("create temp workspace");
        Self { root }
    }

    pub(super) fn write_file(&self, rel: &str, content: &str) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, content).expect("write file");
    }

    pub(super) fn open_db(&self) -> WeavebackDb {
        WeavebackDb::open(self.root.join("weaveback.db")).expect("open sqlite db")
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

