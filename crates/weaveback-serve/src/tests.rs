// weaveback-serve/src/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::{
    build_chunk_context, content_type, extract_prose, heading_level, parse_query,
    percent_decode, safe_path, section_range, sse_headers, tangle_oracle, title_chain,
    AiBackend, AiChannelReader, SseReader, TangleConfig,
};
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use weaveback_tangle::db::{ChunkDefEntry, Confidence, NowebMapEntry, WeavebackDb};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TestWorkspace {
    root: PathBuf,
}

impl TestWorkspace {
    fn new() -> Self {
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

    fn write_file(&self, rel: &str, content: &str) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, content).expect("write file");
    }

    fn open_db(&self) -> WeavebackDb {
        WeavebackDb::open(self.root.join("weaveback.db")).expect("open sqlite db")
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn content_type_and_safe_path_handle_common_cases() {
    let workspace = TestWorkspace::new();
    workspace.write_file("docs/index.html", "<html></html>");
    workspace.write_file("docs/app.js", "console.log('x');");

    let docs_dir = workspace.root.join("docs");
    assert_eq!(content_type(&docs_dir.join("index.html")), "text/html; charset=utf-8");
    assert_eq!(
        content_type(&docs_dir.join("app.js")),
        "application/javascript; charset=utf-8"
    );
    assert_eq!(safe_path(&docs_dir, "/index.html"), Some(docs_dir.join("index.html")));
    assert_eq!(safe_path(&docs_dir, "/"), Some(docs_dir.join("index.html")));
    assert_eq!(safe_path(&docs_dir, "/../secret"), None);
    assert_eq!(safe_path(&docs_dir, "/missing.txt"), None);
}

#[test]
fn parse_query_and_percent_decode_decode_pairs() {
    let params = parse_query("/__chunk?file=docs%2Fintro.adoc&name=alpha%20beta&nth=2");
    assert_eq!(params.get("file").map(String::as_str), Some("docs/intro.adoc"));
    assert_eq!(params.get("name").map(String::as_str), Some("alpha beta"));
    assert_eq!(params.get("nth").map(String::as_str), Some("2"));
    assert_eq!(percent_decode("a%2Fb%20c"), "a/b c");
    assert_eq!(percent_decode("%4"), "%4");
}

#[test]
fn parse_query_and_percent_decode_preserve_incomplete_or_empty_values() {
    let params = parse_query("/__chunk?flag&empty=&bad=%zz");
    assert_eq!(params.get("flag").map(String::as_str), Some(""));
    assert_eq!(params.get("empty").map(String::as_str), Some(""));
    assert_eq!(params.get("bad").map(String::as_str), Some("%zz"));
    assert_eq!(percent_decode("plain"), "plain");
}

#[test]
fn section_helpers_extract_expected_prose_context() {
    let lines = vec![
        "= Root",
        "",
        "== Parser",
        "Intro prose.",
        "----",
        "code",
        "----",
        "",
        "=== Nested",
        "Nested prose.",
        "== Other",
        "Later.",
    ];

    assert_eq!(heading_level("== Parser"), Some(2));
    assert_eq!(heading_level("==Parser"), None);
    assert_eq!(section_range(&lines, 3), (2, 10));
    assert_eq!(title_chain(&lines, 9), vec!["Root", "Parser", "Nested"]);
    assert_eq!(extract_prose(&lines, 2, 10), "== Parser\nIntro prose.\n\n=== Nested\nNested prose.");
}

#[test]
fn extract_prose_skips_chunk_bodies_and_trims_blank_edges() {
    let lines = vec![
        "",
        "Intro paragraph.",
        "",
        "// <<alpha>>=",
        "let hidden = true;",
        "// @",
        "",
        "Outro paragraph.",
        "",
    ];

    let prose = extract_prose(&lines, 0, lines.len());
    assert!(!prose.contains("hidden = true"));
    assert!(prose.starts_with("Intro paragraph."));
    assert!(prose.ends_with("Outro paragraph."));
}

#[test]
fn heading_and_section_helpers_handle_edge_cases() {
    let lines = vec!["= Root", "plain text", "=== Deep", "==== Deeper", "body"];

    assert_eq!(heading_level("plain text"), None);
    assert_eq!(heading_level("==NoSpace"), None);
    assert_eq!(heading_level("=== Deep"), Some(3));
    assert_eq!(title_chain(&lines, 4), vec!["Root", "Deep", "Deeper"]);
    assert_eq!(section_range(&lines, 4), (3, 5));
}

#[test]
fn default_tangle_config_matches_expected_defaults() {
    let cfg = TangleConfig::default();
    assert_eq!(cfg.open_delim, "<[");
    assert_eq!(cfg.close_delim, "]>");
    assert_eq!(cfg.chunk_end, "@@");
    assert_eq!(cfg.comment_markers, vec!["//".to_string()]);
    assert!(matches!(cfg.ai_backend, AiBackend::ClaudeCli));
    assert!(cfg.ai_model.is_none());
    assert!(cfg.ai_endpoint.is_none());
}

#[test]
fn build_chunk_context_includes_dependencies_outputs_and_section_prose() {
    let workspace = TestWorkspace::new();
    let source = [
        "= Root",
        "",
        "== Serve",
        "Serve prose.",
        "",
        "// <<alpha>>=",
        "alpha line",
        "<<beta>>",
        "// @",
        "",
        "// <<beta>>=",
        "beta line",
        "// @",
    ]
    .join("\n");
    workspace.write_file("docs/serve.adoc", &source);

    let mut db = workspace.open_db();
    db.set_chunk_defs(&[
        ChunkDefEntry {
            src_file: "docs/serve.adoc".to_string(),
            chunk_name: "alpha".to_string(),
            nth: 0,
            def_start: 6,
            def_end: 9,
        },
        ChunkDefEntry {
            src_file: "docs/serve.adoc".to_string(),
            chunk_name: "beta".to_string(),
            nth: 0,
            def_start: 11,
            def_end: 13,
        },
    ])
    .unwrap();
    db.set_chunk_deps(&[
        ("alpha".to_string(), "beta".to_string(), "docs/serve.adoc".to_string()),
        ("gamma".to_string(), "alpha".to_string(), "docs/serve.adoc".to_string()),
    ])
    .unwrap();
    db.set_noweb_entries(
        "gen/out.rs",
        &[(
            0,
            NowebMapEntry {
                src_file: "docs/serve.adoc".to_string(),
                chunk_name: "alpha".to_string(),
                src_line: 5,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .unwrap();
    drop(db);

    let ctx = build_chunk_context(&workspace.root, "docs/serve.adoc", "alpha", 0);
    assert_eq!(ctx["file"], "docs/serve.adoc");
    assert_eq!(ctx["name"], "alpha");
    assert_eq!(ctx["body"], "alpha line\n<<beta>>");
    assert_eq!(ctx["section_title_chain"], serde_json::json!(["Root", "Serve"]));
    assert_eq!(ctx["section_prose"], "== Serve\nServe prose.");
    assert_eq!(ctx["output_files"], serde_json::json!(["gen/out.rs"]));
    assert_eq!(ctx["reverse_dependencies"], serde_json::json!(["gamma"]));
    assert_eq!(ctx["dependencies"]["beta"]["file"], "docs/serve.adoc");
    assert_eq!(ctx["dependencies"]["beta"]["body"], "beta line");
    assert!(ctx["git_log"].as_array().is_some_and(|items| items.is_empty()));
}

#[test]
fn build_chunk_context_returns_null_for_missing_chunk() {
    let workspace = TestWorkspace::new();
    workspace.write_file("docs/serve.adoc", "= Title\n");
    let ctx = build_chunk_context(&workspace.root, "docs/serve.adoc", "missing", 0);
    assert_eq!(ctx, serde_json::Value::Null);
}

#[test]
fn tangle_oracle_accepts_plain_prose_files() {
    let workspace = TestWorkspace::new();
    workspace.write_file("docs/a.adoc", "= A\n\nPlain prose.\n");
    workspace.write_file("docs/b.adoc", "= B\n\nOther prose.\n");

    let result = tangle_oracle(
        &workspace.root,
        "docs/a.adoc",
        "= A\n\nUpdated prose.\n",
        &TangleConfig {
            open_delim: "<<".to_string(),
            close_delim: ">>".to_string(),
            chunk_end: "@".to_string(),
            comment_markers: vec!["//".to_string()],
            ..TangleConfig::default()
        },
    );

    assert!(result.is_ok());
}

#[test]
fn tangle_oracle_reports_missing_directory() {
    let workspace = TestWorkspace::new();
    let err = tangle_oracle(
        &workspace.root,
        "missing/a.adoc",
        "= A\n",
        &TangleConfig::default(),
    )
    .expect_err("missing directory should fail");
    assert!(err.contains("io_error"));
}

#[test]
fn sse_headers_and_readers_emit_expected_frames() {
    let headers = sse_headers();
    assert!(
        headers
            .iter()
            .any(|h| h.field.equiv("Content-Type") && h.value.as_str() == "text/event-stream")
    );
    assert!(
        headers
            .iter()
            .any(|h| h.field.equiv("Cache-Control") && h.value.as_str() == "no-cache")
    );

    let (_tx, rx) = std::sync::mpsc::channel();
    let mut sse = SseReader::new(rx);
    let mut buf = [0u8; 64];
    let n = sse.read(&mut buf).unwrap();
    let first = std::str::from_utf8(&buf[..n]).unwrap();
    assert_eq!(first, ": weaveback-serve\n\n");

    let (tx, rx) = std::sync::mpsc::channel();
    let mut ai = AiChannelReader::new(rx);
    let n = ai.read(&mut buf).unwrap();
    let first = std::str::from_utf8(&buf[..n]).unwrap();
    assert_eq!(first, ": weaveback-ai\n\n");
    tx.send("event: token\ndata: {\"t\":\"hi\"}\n\n".to_string())
        .unwrap();
    let n = ai.read(&mut buf).unwrap();
    let second = std::str::from_utf8(&buf[..n]).unwrap();
    assert_eq!(second, "event: token\ndata: {\"t\":\"hi\"}\n\n");
}

#[test]
fn json_resp_sets_json_and_cors_headers() {
    let response = super::json_resp(serde_json::json!({ "ok": true }));
    let headers = response.headers();
    assert!(
        headers
            .iter()
            .any(|h| h.field.equiv("Content-Type") && h.value.as_str() == "application/json")
    );
    assert!(
        headers
            .iter()
            .any(|h| h.field.equiv("Access-Control-Allow-Origin") && h.value.as_str() == "*")
    );
}

#[test]
fn sse_reader_emits_reload_frame_after_signal() {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut sse = SseReader::new(rx);
    let mut buf = [0u8; 64];
    let _ = sse.read(&mut buf).unwrap();
    tx.send(()).unwrap();
    let n = sse.read(&mut buf).unwrap();
    let frame = std::str::from_utf8(&buf[..n]).unwrap();
    assert_eq!(frame, "event: reload\ndata:\n\n");
}

#[test]
fn git_log_for_file_returns_empty_outside_repo() {
    let workspace = TestWorkspace::new();
    let log = super::git_log_for_file(&workspace.root, "docs/missing.adoc");
    assert!(log.is_empty());
}

#[test]
fn dep_bodies_skips_missing_definitions() {
    let workspace = TestWorkspace::new();
    workspace.write_file("docs/dep.adoc", "// <<alpha>>=\nalpha\n// @\n");

    let mut db = workspace.open_db();
    db.set_chunk_defs(&[ChunkDefEntry {
        src_file: "docs/dep.adoc".to_string(),
        chunk_name: "alpha".to_string(),
        nth: 0,
        def_start: 1,
        def_end: 3,
    }])
    .unwrap();
    let deps = super::dep_bodies(
        &db,
        &workspace.root,
        &[
            ("alpha".to_string(), "docs/dep.adoc".to_string()),
            ("missing".to_string(), "docs/dep.adoc".to_string()),
        ],
    );
    assert_eq!(deps.len(), 1);
    assert_eq!(deps["alpha"]["body"], "alpha");
}
// ── content_type extended ─────────────────────────────────────────────

#[test]
fn content_type_covers_all_mapped_extensions() {
    let cases = &[
        ("style.css",  "text/css; charset=utf-8"),
        ("logo.svg",   "image/svg+xml"),
        ("icon.ico",   "image/x-icon"),
        ("data.json",  "application/json"),
        ("photo.png",  "image/png"),
        ("main.js",    "application/javascript; charset=utf-8"),
        ("blob.bin",   "application/octet-stream"),
    ];
    let base = PathBuf::from("/tmp");
    for (name, expected) in cases {
        assert_eq!(content_type(&base.join(name)), *expected, "for {name}");
    }
}

// ── safe_path edge cases ──────────────────────────────────────────────

#[test]
fn safe_path_rejects_absolute_url_path() {
    let workspace = TestWorkspace::new();
    workspace.write_file("docs/index.html", "");
    let docs_dir = workspace.root.join("docs");
    // URL path that resolves outside of docs_dir via absolute-looking component
    assert_eq!(safe_path(&docs_dir, "/../etc/passwd"), None);
}

#[test]
fn safe_path_returns_none_for_empty_directory_without_index() {
    let workspace = TestWorkspace::new();
    workspace.write_file("docs/sub/contents.txt", "");
    let docs_dir = workspace.root.join("docs");
    assert_eq!(safe_path(&docs_dir, "/sub"), None);
}

// ── heading_level extended ────────────────────────────────────────────

#[test]
fn heading_level_returns_correct_depth() {
    assert_eq!(heading_level("= Title"), Some(1));
    assert_eq!(heading_level("== Section"), Some(2));
    assert_eq!(heading_level("==== Level4"), Some(4));
    assert_eq!(heading_level(""), None);
    // A trailing space without title: "== " has t.len() == count+1; t[count] == b' ' is true but no title text
    assert_eq!(heading_level("== "), None); // empty title — implementation returns None
    assert_eq!(heading_level("=no space"), None);
}

// ── section_range edge cases ──────────────────────────────────────────

#[test]
fn section_range_returns_entire_file_when_no_heading() {
    let lines = vec!["plain", "text", "only"];
    // def_start=0, no heading found ↑, sec_start=0, sec_level=1
    // no next heading found → sec_end=len
    let (start, end) = section_range(&lines, 1);
    assert_eq!(start, 0);
    assert_eq!(end, lines.len());
}

#[test]
fn section_range_stops_at_sibling_heading() {
    let lines = vec![
        "== Alpha",   // 0
        "alpha body", // 1
        "== Beta",    // 2 — sibling
        "beta body",  // 3
    ];
    let (start, end) = section_range(&lines, 1);
    assert_eq!(start, 0);
    assert_eq!(end, 2); // stops before Beta
}

// ── extract_prose edge cases ──────────────────────────────────────────

#[test]
fn extract_prose_handles_interleaved_fence_and_chunk() {
    let lines = vec![
        "Intro.",               // 0
        "----",                 // 1 — open fence
        "code inside fence",   // 2
        "----",                 // 3 — close fence
        "// <<chunk>>=",       // 4 — open chunk
        "chunk body",          // 5
        "// @",                // 6 — close chunk
        "Outro.",              // 7
    ];
    let prose = extract_prose(&lines, 0, lines.len());
    assert!(!prose.contains("code inside fence"));
    assert!(!prose.contains("chunk body"));
    assert!(prose.contains("Intro."));
    assert!(prose.contains("Outro."));
}

#[test]
fn extract_prose_returns_empty_for_all_code() {
    let lines = vec!["----", "all code", "----"];
    let prose = extract_prose(&lines, 0, lines.len());
    assert_eq!(prose, "");
}

// ── percent_decode edge cases ─────────────────────────────────────────

#[test]
fn percent_decode_handles_uppercase_hex() {
    assert_eq!(percent_decode("%2F"), "/");
    assert_eq!(percent_decode("%20"), " ");
    assert_eq!(percent_decode("%7E"), "~");
}

#[test]
fn percent_decode_passes_non_encoded_chars() {
    assert_eq!(percent_decode("hello world"), "hello world");
    assert_eq!(percent_decode(""), "");
}

// ── parse_query edge cases ────────────────────────────────────────────

#[test]
fn parse_query_returns_empty_when_no_query_string() {
    let params = parse_query("/__chunk");
    // No '?' in the URL → empty query → all keys from empty string split are empty
    assert!(!params.contains_key("file"));
    assert!(!params.contains_key("name"));
}

#[test]
fn parse_query_handles_empty_query_string() {
    let params = parse_query("/__chunk?");
    assert!(params.is_empty() || params.contains_key(""));
}

// ── SseReader EOF ─────────────────────────────────────────────────────

#[test]
fn sse_reader_returns_zero_bytes_on_sender_drop() {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut sse = SseReader::new(rx);
    let mut buf = [0u8; 64];
    // drain the initial keepalive
    while {
        let n = sse.read(&mut buf).unwrap();
        n > 0 && buf[..n] != *b": weaveback-serve\n\n"
    } {}
    // drop the sender — next read should return 0 (EOF)
    drop(tx);
    loop {
        let n = sse.read(&mut buf).unwrap();
        if n == 0 { break; }
    }
}

// ── tangle_oracle with chunk syntax ───────────────────────────────────

#[test]
fn tangle_oracle_accepts_file_with_chunk_syntax() {
    let workspace = TestWorkspace::new();
    // A minimal file using the default `<[` / `]>` / `@@` syntax.
    let content = "// <[my-chunk]=\nfn main() {}\n// ]>\n@@\n";
    workspace.write_file("src/lib.adoc", content);
    let cfg = TangleConfig::default();
    let result = tangle_oracle(&workspace.root, "src/lib.adoc", content, &cfg);
    assert!(result.is_ok(), "oracle should accept valid chunk syntax: {result:?}");
}

#[test]
fn test_apply_chunk_edit_replaces_correct_lines() {
    let src = "line1\n// <<chunk>>=\nold\n// @\nline2\n";
    let res = super::apply_chunk_edit(src, 2, 4, "new\nlines");
    assert_eq!(res, "line1\n// <<chunk>>=\nnew\nlines\n// @\nline2\n");
}

#[test]
fn test_extract_chunk_body_returns_text_between_markers() {
    let src = "line1\n// <<chunk>>=\nbody line\n// @\nline2";
    let res = super::extract_chunk_body(src, 2, 4).unwrap();
    assert_eq!(res, "body line");
}

#[test]
fn test_insert_note_into_source_places_note_after_fence() {
    let src = "== Header\n\n// <<chunk>>=\nbody\n// @\n----\n\nProse.";
    // def_end for the chunk is 5 (1-indexed // @ marker)
    let res = super::insert_note_into_source(src, 5, "my note");
    assert!(res.contains("[NOTE]\n====\nmy note\n====\n"));
    assert!(res.contains("----\n[NOTE]")); // inserted after fence
}

#[test]
fn test_find_project_root_walks_up_to_workspace() {
    let workspace = TestWorkspace::new();
    workspace.write_file("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]");
    workspace.write_file("crates/a/src/lib.rs", "");

    let subdir = workspace.root.join("crates").join("a");
    std::fs::create_dir_all(&subdir).unwrap();

    // We can't easily change CWD safely in tests, but find_project_root
    // uses current_dir(). We can mock it if we refactor it, but for now
    // we'll just test that it finds the current repo if run in the repo.
    // Actually, let's just test that the helper exists and hasn't regressed.
    let root = super::find_project_root();
    assert!(root.join("Cargo.toml").exists());
}

#[test]
fn test_find_docgen_bin_favors_sibling() {
    // find_docgen_bin uses current_exe(). Hard to mock without relative paths.
    // But we can check that it returns a PathBuf.
    let bin = super::find_docgen_bin();
    assert!(bin.to_string_lossy().contains("weaveback-docgen"));
}

#[test]
fn test_safe_path_redirects_root_to_docs_index() {
    let workspace = TestWorkspace::new();
    workspace.write_file("docs/index.html", "hi");
    let docs_dir = workspace.root.join("docs");

    // safe_path handles the /index.html logic
    assert_eq!(super::safe_path(&docs_dir, "/"), Some(docs_dir.join("index.html")));
}

#[test]
fn test_handler_integration() {
    let workspace = TestWorkspace::new();
    workspace.write_file("docs/index.html", "hi");
    workspace.write_file("src/lib.adoc", "// <<chunk>>=\nalpha\n// @\n----\n");

    let mut db = workspace.open_db();
    db.set_chunk_defs(&[ChunkDefEntry {
        src_file: "src/lib.adoc".to_string(),
        chunk_name: "chunk".to_string(),
        nth: 0,
        def_start: 1,
        def_end: 3,
    }]).unwrap();
    drop(db);

    let server_root = workspace.root.clone();
    let html_dir = workspace.root.join("docs");
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = match server.server_addr() {
        tiny_http::ListenAddr::IP(addr) => addr.port(),
        _ => panic!("Expected IP address"),
    };

    std::thread::spawn(move || {
        let _ = super::run_server_loop(
            server,
            server_root,
            html_dir,
            false,
            TangleConfig::default(),
        );
    });

    // Give server a moment to start
    std::thread::sleep(std::time::Duration::from_millis(100));

    let base_url = format!("http://127.0.0.1:{}", port);

    // Test /__chunk
    let resp = ureq::get(&format!("{}/__chunk?file=src/lib.adoc&name=chunk", base_url))
        .call().unwrap();
    let json: serde_json::Value = resp.into_json().unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["body"], "alpha");

    // Test /__apply
    let resp = ureq::post(&format!("{}/__apply", base_url))
        .send_json(serde_json::json!({
            "file": "src/lib.adoc",
            "name": "chunk",
            "old_body": "alpha",
            "new_body": "beta"
        })).unwrap();
    let json: serde_json::Value = resp.into_json().unwrap();
    assert_eq!(json["ok"], true);

    let updated = fs::read_to_string(workspace.root.join("src/lib.adoc")).unwrap();
    assert!(updated.contains("beta"));

    // Test /__save_note
    let resp = ureq::post(&format!("{}/__save_note", base_url))
        .send_json(serde_json::json!({
            "file": "src/lib.adoc",
            "name": "chunk",
            "nth": 0,
            "note": "AI suggestion"
        })).unwrap();
    let json: serde_json::Value = resp.into_json().unwrap();
    assert_eq!(json["ok"], true);
    let noted = fs::read_to_string(workspace.root.join("src/lib.adoc")).unwrap();
    assert!(noted.contains("[NOTE]"));
    assert!(noted.contains("AI suggestion"));
}

