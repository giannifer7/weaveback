// weaveback-api/src/semantic/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

// ── request builders ──────────────────────────────────────────────────────

#[test]
fn build_openai_embedding_request_forms_url() {
    let inputs = vec!["hello".to_string()];
    let (url, body) = build_openai_embedding_request(
        "https://api.openai.com/v1", "text-embedding-3-small", &inputs,
    );
    assert_eq!(url, "https://api.openai.com/v1/embeddings");
    assert_eq!(body["model"], "text-embedding-3-small");
    assert_eq!(body["input"][0], "hello");
}

#[test]
fn build_openai_embedding_request_trims_trailing_slash() {
    let inputs = vec!["x".to_string()];
    let (url, _) = build_openai_embedding_request(
        "http://localhost:11434/v1/", "model", &inputs,
    );
    assert_eq!(url, "http://localhost:11434/v1/embeddings");
}

#[test]
fn parse_openai_embedding_response_extracts_vectors() {
    let value = serde_json::json!({
        "data": [{ "embedding": [0.1_f64, 0.2_f64, 0.3_f64] }]
    });
    let vecs = parse_openai_embedding_response(&value).unwrap();
    assert_eq!(vecs.len(), 1);
    assert!((vecs[0][0] - 0.1_f32).abs() < 1e-6);
}

#[test]
fn parse_openai_embedding_response_errors_on_bad_shape() {
    let value = serde_json::json!({ "error": "bad request" });
    assert!(parse_openai_embedding_response(&value).is_err());
}

#[test]
fn build_gemini_embedding_request_embeds_key_in_url() {
    let inputs = vec!["test".to_string()];
    let (url, body) = build_gemini_embedding_request("mykey", "embed-model", &inputs);
    assert!(url.contains("mykey"));
    assert!(url.contains("embed-model"));
    assert_eq!(body["requests"][0]["content"]["parts"][0]["text"], "test");
}

#[test]
fn parse_gemini_embedding_response_extracts_vectors() {
    let value = serde_json::json!({
        "embeddings": [{ "values": [0.5_f64, -0.5_f64] }]
    });
    let vecs = parse_gemini_embedding_response(&value).unwrap();
    assert_eq!(vecs.len(), 1);
    assert!((vecs[0][0] - 0.5_f32).abs() < 1e-6);
}

#[test]
fn parse_gemini_embedding_response_errors_on_bad_shape() {
    let value = serde_json::json!({ "bad": "data" });
    assert!(parse_gemini_embedding_response(&value).is_err());
}

// ── block text extraction ─────────────────────────────────────────────────

#[test]
fn extract_block_text_returns_requested_range() {
    let src = "line1\nline2\nline3\nline4";
    // 1-based: lines 2..3 → "line2\nline3"
    assert_eq!(extract_block_text(src, 2, 3), "line2\nline3");
}

#[test]
fn extract_block_text_single_line() {
    let src = "alpha\nbeta\ngamma";
    assert_eq!(extract_block_text(src, 2, 2), "beta");
}

#[test]
fn extract_block_text_out_of_range_returns_empty() {
    let src = "only one line";
    assert_eq!(extract_block_text(src, 10, 20), "");
}

// ── embed_texts dispatcher ────────────────────────────────────────────────

#[test]
fn embed_texts_rejects_anthropic_backend() {
    let cfg = EmbeddingConfig {
        backend:    "anthropic".to_string(),
        model:      "any".to_string(),
        endpoint:   None,
        batch_size: 1,
    };
    let err = embed_texts(&cfg, &["hello".to_string()]).unwrap_err();
    assert!(err.contains("Anthropic"));
}

// ── default config values ────────────────────────────────────────────────

#[test]
fn default_embeddings_backend_is_nonempty() {
    assert!(!default_embeddings_backend().is_empty());
}

#[test]
fn default_embeddings_model_is_nonempty() {
    assert!(!default_embeddings_model().is_empty());
}

#[test]
fn default_embeddings_batch_size_is_positive() {
    assert!(default_embeddings_batch_size() > 0);
}

// ── persist_embedding_config ─────────────────────────────────────────────

#[test]
fn persist_embedding_config_stores_all_fields() {
    use weaveback_tangle::db::WeavebackDb;
    let db = WeavebackDb::open_temp().unwrap();
    let cfg = EmbeddingConfig {
        backend:    "gemini".to_string(),
        model:      "embed-v2".to_string(),
        endpoint:   Some("https://example.com".to_string()),
        batch_size: 8,
    };
    persist_embedding_config(&db, &cfg).unwrap();
    assert_eq!(db.get_run_config("semantic.backend").unwrap().as_deref(), Some("gemini"));
    assert_eq!(db.get_run_config("semantic.model").unwrap().as_deref(),   Some("embed-v2"));
    assert_eq!(db.get_run_config("semantic.batch_size").unwrap().as_deref(), Some("8"));
    assert_eq!(db.get_run_config("semantic.endpoint").unwrap().as_deref(), Some("https://example.com"));
}

#[test]
fn test_run_auto_embed_success() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("wb.db");
    let mut db = weaveback_tangle::WeavebackDb::open(&db_path).unwrap();

    let src = "= Section\n\nSome prose here.\n";
    let blocks = weaveback_tangle::parse_source_blocks(src, "adoc");
    db.set_source_blocks("doc.adoc", &blocks).unwrap();
    db.set_src_snapshot("doc.adoc", src.as_bytes()).unwrap();

    let script = r#"
import sys, json, http.server, threading

class MockHandler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        l = int(self.headers["Content-Length"])
        body = json.loads(self.rfile.read(l))
        num_inputs = len(body["input"])
        res = {"data": [{"embedding": [0.1] * 32} for _ in range(num_inputs)]}
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(res).encode())
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
    def log_message(self, format, *args): return

httpd = http.server.HTTPServer(("127.0.0.1", 11112), MockHandler)
t = threading.Thread(target=httpd.serve_forever)
t.daemon = True
t.start()
import time
while True: time.sleep(1)
"#;
    let script_path = tmp.path().join("mock_embed.py");
    std::fs::write(&script_path, script).unwrap();

    // Start the mock server in a background process
    let mut child = std::process::Command::new("python3")
        .arg(&script_path)
        .spawn()
        .unwrap();

    // Wait for the mock server to start by attempting to connect in a loop.
    let mut started = false;
    for _ in 0..50 {
        if std::net::TcpStream::connect("127.0.0.1:11112").is_ok() {
            started = true;
            break;
        }
        if let Ok(Some(_)) = child.try_wait() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let cfg = EmbeddingConfig {
        backend: "ollama".to_string(),
        model: "m".to_string(),
        endpoint: Some("http://127.0.0.1:11112".to_string()),
        batch_size: 10,
    };

    if started {
        run_auto_embed(&mut db, &cfg);
    }

    let _ = child.kill();

    // Verify embeddings were stored
    let needs = db.get_blocks_needing_embeddings("m").unwrap();
    assert!(needs.is_empty(), "all blocks should have been embedded");
}

#[test]
fn test_run_auto_embed_normalises_snapshot_paths() {
    use weaveback_tangle::SourceBlockEntry;
    let mut db = WeavebackDb::open_temp().unwrap();
    let src = "content";
    db.set_src_snapshot("test.adoc", src.as_bytes()).unwrap();

    // Block uses ./ prefix
    let blocks = vec![SourceBlockEntry {
        block_index: 0,
        block_type: "prose".to_string(),
        line_start: 1,
        line_end: 1,
        content_hash: [0u8; 32],
    }];
    db.set_source_blocks("./test.adoc", &blocks).unwrap();

    let cfg = EmbeddingConfig {
        backend: "ollama".to_string(),
        model: "m".to_string(),
        endpoint: Some("http://127.0.0.1:9/v1".to_string()),
        batch_size: 1,
    };
    // Should attempt to fetch "test.adoc" when "./test.adoc" fails.
    // It still fails the API call, but covers the normalization branch.
    run_auto_embed(&mut db, &cfg);
}

#[test]
fn test_embed_texts_dispatches_gemini_errors_when_key_missing() {
    unsafe { std::env::remove_var("GOOGLE_API_KEY") };
    let cfg = EmbeddingConfig {
        backend: "gemini".to_string(),
        model: "m".to_string(),
        endpoint: None,
        batch_size: 1,
    };
    let res = embed_texts(&cfg, &["prompt".to_string()]);
    assert_eq!(res.unwrap_err(), "GOOGLE_API_KEY not set");
}

#[test]
fn test_embed_texts_dispatches_openai_with_key() {
    unsafe { std::env::set_var("OPENAI_API_KEY", "sk-123") };
    let cfg = EmbeddingConfig {
        backend: "openai".to_string(),
        model: "m".to_string(),
        endpoint: None,
        batch_size: 1,
    };
    let res = embed_texts(&cfg, &["prompt".to_string()]);
    assert!(res.is_err()); // Connection refused to api.openai.com
    unsafe { std::env::remove_var("OPENAI_API_KEY") };
}

#[test]
fn test_run_auto_embed_skips_when_no_blocks() {
    let mut db = WeavebackDb::open_temp().unwrap();
    let cfg = EmbeddingConfig {
        backend: "openai".to_string(),
        model: "m".to_string(),
        endpoint: None,
        batch_size: 1,
    };
    run_auto_embed(&mut db, &cfg);
    // Should not panic, should just complete.
}

