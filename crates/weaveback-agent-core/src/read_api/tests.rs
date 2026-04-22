// weaveback-agent-core/src/read_api/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::{
    chunk_context, extract_prose, heading_level, prepare_fts_query, reciprocal_rank, search,
    section_range, title_chain, trace,
};
use crate::workspace::WorkspaceConfig;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use weaveback_tangle::block_parser::SourceBlockEntry;
use weaveback_tangle::db::{ChunkDefEntry, Confidence, NowebMapEntry, WeavebackDb};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TestWorkspace {
    root: PathBuf,
    db_path: PathBuf,
    gen_dir: PathBuf,
}

impl TestWorkspace {
    fn new() -> Self {
        let unique = format!(
            "wb-agent-read-api-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock drifted backwards")
                .as_nanos()
                + u128::from(TEST_COUNTER.fetch_add(1, Ordering::Relaxed))
        );
        let root = std::env::temp_dir().join(unique);
        let gen_dir = root.join("gen");
        let db_path = root.join("weaveback.db");
        fs::create_dir_all(&gen_dir).expect("create temp workspace");
        Self {
            root,
            db_path,
            gen_dir,
        }
    }

    fn config(&self) -> WorkspaceConfig {
        WorkspaceConfig {
            project_root: self.root.clone(),
            db_path: self.db_path.clone(),
            gen_dir: self.gen_dir.clone(),
        }
    }

    fn write_source(&self, rel: &str, content: &str) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create source parent");
        }
        fs::write(path, content).expect("write source");
    }

    fn open_db(&self) -> WeavebackDb {
        WeavebackDb::open(&self.db_path).expect("open sqlite db")
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn block(index: u32, block_type: &str, line_start: u32, line_end: u32) -> SourceBlockEntry {
    SourceBlockEntry {
        block_index: index,
        block_type: block_type.to_string(),
        line_start,
        line_end,
        content_hash: [0u8; 32],
    }
}

fn tags_hash(seed: u8) -> [u8; 32] {
    [seed; 32]
}

#[test]
fn heading_level_detects_valid_headings() {
    assert_eq!(heading_level("= Top"), Some(1));
    assert_eq!(heading_level("=== Nested"), Some(3));
    assert_eq!(heading_level("==== Four  "), Some(4));
}

#[test]
fn heading_level_rejects_non_headings() {
    assert_eq!(heading_level(""), None);
    assert_eq!(heading_level("==="), None);
    assert_eq!(heading_level("==NoSpace"), None);
    assert_eq!(heading_level(" plain text"), None);
}

#[test]
fn section_range_stops_at_same_or_higher_heading() {
    let lines = vec![
        "= Root",
        "",
        "intro",
        "== First",
        "first prose",
        "=== Deeper",
        "nested prose",
        "== Second",
        "second prose",
    ];

    assert_eq!(section_range(&lines, 4), (3, 7));
    assert_eq!(section_range(&lines, 6), (5, 7));
}

#[test]
fn title_chain_tracks_current_breadcrumb() {
    let lines = vec![
        "= Root",
        "intro",
        "== First",
        "text",
        "=== Deeper",
        "body",
        "== Second",
        "more",
    ];

    assert_eq!(title_chain(&lines, 5), vec!["Root", "First", "Deeper"]);
    assert_eq!(title_chain(&lines, 7), vec!["Root", "Second"]);
}

#[test]
fn extract_prose_skips_fenced_blocks_and_trims_edges() {
    let lines = vec![
        "",
        "Intro paragraph.",
        "",
        "----",
        "code line 1",
        "code line 2",
        "----",
        "",
        "Closing note.",
        "",
    ];

    assert_eq!(extract_prose(&lines, 0, lines.len()), "Intro paragraph.\n\n\nClosing note.");
}

#[test]
fn prepare_fts_query_quotes_unsafe_terms_only() {
    assert_eq!(prepare_fts_query("alpha beta"), "alpha beta");
    assert_eq!(prepare_fts_query("error-handling literal"), "\"error-handling\" literal");
    assert_eq!(prepare_fts_query("foo* bar^"), "foo* bar^");
}

#[test]
fn prepare_fts_query_preserves_advanced_queries() {
    assert_eq!(prepare_fts_query("\"exact phrase\""), "\"exact phrase\"");
    assert_eq!(prepare_fts_query("alpha AND beta"), "alpha AND beta");
    assert_eq!(prepare_fts_query("x OR y"), "x OR y");
    assert_eq!(prepare_fts_query("alpha NOT beta"), "alpha NOT beta");
}

#[test]
fn reciprocal_rank_decreases_with_rank() {
    let first = reciprocal_rank(1);
    let second = reciprocal_rank(2);
    let tenth = reciprocal_rank(10);

    assert!(first > second);
    assert!(second > tenth);
    assert!(first > 0.0);
}

#[test]
fn search_returns_ranked_fts_hits_with_tags() {
    let workspace = TestWorkspace::new();
    let source = "= Alpha\n\nWeaveback explores literate systems.\n\nSearchable prose.\n";
    workspace.write_source("docs/alpha.adoc", source);

    let mut db = workspace.open_db();
    db.set_src_snapshot("docs/alpha.adoc", source.as_bytes()).unwrap();
    db.set_source_blocks(
        "docs/alpha.adoc",
        &[
            block(0, "section", 1, 1),
            block(1, "para", 3, 3),
            block(2, "para", 5, 5),
        ],
    )
    .unwrap();
    db.set_block_tags("docs/alpha.adoc", 1, &tags_hash(1), "design,architecture")
        .unwrap();
    db.rebuild_prose_fts(None).unwrap();
    drop(db);

    let hits = search(&workspace.config(), "literate", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].src_file, "docs/alpha.adoc");
    assert_eq!(hits[0].block_type, "para");
    assert_eq!(hits[0].line_start, 3);
    assert_eq!(hits[0].line_end, 3);
    assert!(hits[0].snippet.contains("literate"));
    assert_eq!(hits[0].tags, vec!["design", "architecture"]);
    assert_eq!(hits[0].channels, vec!["fts"]);
    assert!(hits[0].score > 0.0);
}

#[test]
fn chunk_context_reads_source_breadcrumbs_deps_and_outputs() {
    let workspace = TestWorkspace::new();
    let source = [
        "= Root",
        "",
        "== Agent Core",
        "This section explains the alpha chunk.",
        "",
        "// <<alpha>>=",
        "let alpha = 1;",
        "<<beta>>",
        "// @",
        "",
        "== After",
        "Later prose.",
    ]
    .join("\n");
    workspace.write_source("docs/agent.adoc", &source);

    let mut db = workspace.open_db();
    db.set_chunk_defs(&[
        ChunkDefEntry {
            src_file: "docs/agent.adoc".to_string(),
            chunk_name: "alpha".to_string(),
            nth: 0,
            def_start: 6,
            def_end: 9,
        },
        ChunkDefEntry {
            src_file: "docs/agent.adoc".to_string(),
            chunk_name: "beta".to_string(),
            nth: 0,
            def_start: 10,
            def_end: 12,
        },
    ])
    .unwrap();
    db.set_chunk_deps(&[(
        "alpha".to_string(),
        "beta".to_string(),
        "docs/agent.adoc".to_string(),
    )])
    .unwrap();
    db.set_noweb_entries(
        "gen/out.rs",
        &[(
            0,
            NowebMapEntry {
                src_file: "docs/agent.adoc".to_string(),
                chunk_name: "alpha".to_string(),
                src_line: 6,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .unwrap();
    drop(db);

    let ctx = chunk_context(&workspace.config(), "docs/agent.adoc", "alpha", 0).unwrap();
    assert_eq!(ctx.file, "docs/agent.adoc");
    assert_eq!(ctx.name, "alpha");
    assert_eq!(ctx.nth, 0);
    assert_eq!(ctx.section_breadcrumb, vec!["Root", "Agent Core"]);
    assert!(ctx.prose.contains("This section explains the alpha chunk."));
    assert!(!ctx.prose.contains("Later prose."));
    assert_eq!(ctx.body, "let alpha = 1;\n<<beta>>");
    assert_eq!(ctx.direct_dependencies, vec!["beta"]);
    assert_eq!(ctx.outputs, vec!["gen/out.rs"]);
}

#[test]
fn trace_returns_literal_source_location_from_noweb_map() {
    let workspace = TestWorkspace::new();
    let source = "alpha\nbeta\n";
    workspace.write_source("docs/simple.adoc", source);

    let mut db = workspace.open_db();
    db.set_src_snapshot("docs/simple.adoc", source.as_bytes()).unwrap();
    db.set_noweb_entries(
        "gen/out.txt",
        &[(
            0,
            NowebMapEntry {
                src_file: "docs/simple.adoc".to_string(),
                chunk_name: "simple".to_string(),
                src_line: 0,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .unwrap();
    drop(db);

    let traced = trace(&workspace.config(), "gen/out.txt", 1, 1)
        .unwrap()
        .expect("expected a noweb hit");
    assert_eq!(traced.out_file, "gen/out.txt");
    assert_eq!(traced.out_line, 1);
    assert!(traced
        .src_file
        .as_deref()
        .is_some_and(|path| path.ends_with("docs/simple.adoc")));
    assert_eq!(traced.src_line, Some(1));
    assert_eq!(traced.src_col, Some(1));
    assert_eq!(traced.kind.as_deref(), Some("Literal"));
    assert_eq!(traced.macro_name, None);
    assert_eq!(traced.param_name, None);
}

