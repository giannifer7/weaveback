// weaveback-api/src/mcp/tests/helpers.rs
// I'd Really Rather You Didn't edit this generated file.

use super::super::run_mcp;
use std::path::PathBuf;
use weaveback_macro::evaluator::EvalConfig;
use weaveback_tangle::db::WeavebackDb;

// ── Test helpers ──────────────────────────────────────────────────────────

pub(super) struct McpWorkspace {
    pub(super) root: std::path::PathBuf,
}
impl McpWorkspace {
    pub(super) fn new() -> Self {
        let id = format!(
            "wb-mcp-tests-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(id);
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        Self { root }
    }
    pub(super) fn db_path(&self) -> PathBuf { self.root.join("weaveback.db") }
    pub(super) fn gen_dir(&self) -> PathBuf { self.root.join("gen") }
    pub(super) fn open_db(&self) -> WeavebackDb { WeavebackDb::open(self.db_path()).unwrap() }
    pub(super) fn write_file(&self, rel_path: &str, content: &[u8]) {
        let p = self.root.join(rel_path);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }
}
impl Drop for McpWorkspace {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); }
}

pub(super) fn mcp_drive(ws: &McpWorkspace, requests: &str) -> String {
    let reader = std::io::Cursor::new(requests.to_string());
    let mut writer = Vec::new();
    run_mcp(reader, &mut writer, ws.db_path(), ws.gen_dir(), ws.root.clone(), EvalConfig::default()).unwrap();
    String::from_utf8(writer).unwrap()
}

// ── Protocol-level tests ──────────────────────────────────────────────────

// ── LSP integration tests (real rust-analyzer) ────────────────────────
//
// These tests spin up the real `rust-analyzer` binary against the live
// workspace so that the LSP dispatch arms in `run_mcp` are exercised.
// They are marked `#[ignore]` so `cargo test` skips them by default;
// run with `cargo test -- --ignored` to include them.

pub(super) fn mcp_workspace_root() -> std::path::PathBuf {
    let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("Cargo.toml");
        if candidate.exists()
            && let Ok(txt) = std::fs::read_to_string(&candidate)
            && txt.contains("[workspace]") {
            return dir;
        }
        if !dir.pop() { break; }
    }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub(super) fn mcp_rs_path() -> std::path::PathBuf {
    mcp_workspace_root().join("crates/weaveback-api/src/mcp.rs")
}

pub(super) fn lsp_mcp_drive(ws: &McpWorkspace, req: &str) -> String {
    // Use real db so DB presence check passes for LSP tools.
    ws.open_db();
    mcp_drive(ws, req)
}

