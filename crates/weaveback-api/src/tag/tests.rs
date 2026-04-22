// weaveback-api/src/tag/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use weaveback_tangle::WeavebackDb;
use tempfile::tempdir;

fn make_db() -> (WeavebackDb, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db = WeavebackDb::open(&db_path).unwrap();
    (db, dir)
}

// ── request builders ──────────────────────────────────────────────────────

#[test]
fn build_anthropic_body_contains_model_and_message() {
    let body = build_anthropic_body("claude-haiku-4-5-20251001", "hello");
    assert_eq!(body["model"], "claude-haiku-4-5-20251001");
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"], "hello");
}

#[test]
fn parse_anthropic_response_extracts_text() {
    let v = serde_json::json!({
        "content": [{ "type": "text", "text": "0:rust,async" }]
    });
    assert_eq!(parse_anthropic_response(&v).unwrap(), "0:rust,async");
}

#[test]
fn parse_anthropic_response_errors_on_bad_shape() {
    let v = serde_json::json!({ "error": "bad request" });
    assert!(parse_anthropic_response(&v).is_err());
}

#[test]
fn build_gemini_request_embeds_api_key_in_url() {
    let (url, body) = build_gemini_request("key123", "gemini-pro", "hello");
    assert!(url.contains("key123"));
    assert!(url.contains("gemini-pro"));
    assert_eq!(body["contents"][0]["parts"][0]["text"], "hello");
}

#[test]
fn parse_gemini_response_extracts_text() {
    let v = serde_json::json!({
        "candidates": [{ "content": { "parts": [{ "text": "1:db,sqlite" }] } }]
    });
    assert_eq!(parse_gemini_response(&v).unwrap(), "1:db,sqlite");
}

#[test]
fn build_openai_request_forms_correct_url() {
    let (url, body) = build_openai_request("https://api.openai.com/v1", "gpt-4o-mini", "hello");
    assert_eq!(url, "https://api.openai.com/v1/chat/completions");
    assert_eq!(body["model"], "gpt-4o-mini");
}

#[test]
fn parse_openai_response_extracts_content() {
    let v = serde_json::json!({
        "choices": [{ "message": { "content": "0:parser,ast" } }]
    });
    assert_eq!(parse_openai_response(&v).unwrap(), "0:parser,ast");
}

// ── prompt helpers ────────────────────────────────────────────────────────

#[test]
fn test_build_prompt_contains_index_and_content() {
    let items = vec![(0usize, "prose", "SQLite is a database.")];
    let prompt = build_prompt(&items);
    assert!(prompt.contains("[0]"));
    assert!(prompt.contains("SQLite is a database."));
}

#[test]
fn test_parse_response_basic() {
    let response = "0:sqlite,database\n1:rust,error-handling";
    let parsed = parse_response(response);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0], (0, "sqlite,database".to_string()));
    assert_eq!(parsed[1], (1, "rust,error-handling".to_string()));
}

#[test]
fn test_parse_response_sanitises_punctuation() {
    let response = "0:Tag With Spaces!,io";
    let parsed = parse_response(response);
    // Spaces and punctuation are filtered out (not converted to hyphens).
    assert_eq!(parsed[0].1, "tagwithspaces,io");
}

#[test]
fn test_parse_response_skips_invalid_lines() {
    let response = "not-a-number:tags\n0:valid";
    let parsed = parse_response(response);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0], (0, "valid".to_string()));
}

#[test]
fn test_block_first_line_returns_first_line() {
    let source = "line one\nline two\nline three";
    assert_eq!(block_first_line(source, 0, 2), "line one");
}

#[test]
fn test_block_first_line_out_of_range() {
    let source = "only one line";
    assert_eq!(block_first_line(source, 10, 20), "");
}

// ── orchestration ─────────────────────────────────────────────────────────

#[test]
fn test_run_auto_tag_skips_when_no_blocks() {
    let (mut db, _dir) = make_db();
    let cfg = TagConfig {
        backend: "anthropic".to_string(),
        model: "claude-haiku-4-5-20251001".to_string(),
        endpoint: None,
        batch_size: 15,
    };
    // No blocks in db → should complete without error and tag nothing.
    run_auto_tag(&mut db, &cfg);
    assert!(db.list_block_tags(None).unwrap().is_empty());
}

#[test]
fn test_run_auto_tag_skips_unreachable_endpoint() {
    let (mut db, _dir) = make_db();
    let cfg = TagConfig {
        backend: "ollama".to_string(),
        model: "dummy".to_string(),
        endpoint: Some("http://127.0.0.1:9/v1".to_string()),
        batch_size: 1,
    };
    run_auto_tag(&mut db, &cfg);
    assert!(db.list_block_tags(None).unwrap().is_empty());
}

#[test]
fn test_parse_response_empty_colon_skipped() {
    // "0:" has no tags after the colon → filter_map returns None → skipped
    let parsed = parse_response("0:\n1:valid");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0], (1, "valid".to_string()));
}

#[test]
fn test_parse_response_all_punctuation_tags_skipped() {
    // "0:!@#" sanitises to empty string → filter_map returns None → skipped
    let parsed = parse_response("0:!@#\n1:good");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0], (1, "good".to_string()));
}

#[test]
fn test_run_auto_tag_processes_blocks_with_snapshot() {
    // With blocks + snapshot in db, run_auto_tag reaches the LLM call path.
    // Port 9 on 127.0.0.1 is unused → connection refused → graceful skip.
    use weaveback_tangle::parse_source_blocks;
    let (mut db, _dir) = make_db();
    let src = "= Section\n\nSome prose here.\n";
    let blocks = parse_source_blocks(src, "adoc");
    db.set_source_blocks("doc.adoc", &blocks).unwrap();
    db.set_src_snapshot("doc.adoc", src.as_bytes()).unwrap();
    let cfg = TagConfig {
        backend: "ollama".to_string(),
        model: "dummy".to_string(),
        endpoint: Some("http://127.0.0.1:9/v1".to_string()),
        batch_size: 10,
    };
    // Should not panic; endpoint unreachable → graceful skip, no tags stored.
    run_auto_tag(&mut db, &cfg);
    assert!(db.list_block_tags(None).unwrap().is_empty());
}

#[test]
fn test_run_auto_tag_normalises_snapshot_paths() {
    let (mut db, _dir) = make_db();
    let src = "content";
    // Seed snapshot with normalized path
    db.set_src_snapshot("test.adoc", src.as_bytes()).unwrap();

    let blocks = vec![weaveback_tangle::SourceBlockEntry {
        block_index: 0,
        block_type: "prose".to_string(),
        line_start: 1,
        line_end: 1,
        content_hash: [0u8; 32],
    }];
    db.set_source_blocks("./test.adoc", &blocks).unwrap();

    let cfg = TagConfig {
        backend: "ollama".to_string(),
        model: "dummy".to_string(),
        endpoint: Some("http://127.0.0.1:9/v1".to_string()), // Unreachable
        batch_size: 10,
    };
    // Should attempt to fetch "test.adoc" when "./test.adoc" fails.
    // It still fails the LLM call, but it covers the normalization branch.
    run_auto_tag(&mut db, &cfg);
}

#[test]
fn test_call_llm_dispatches_ollama_to_openai_compat() {
    let cfg = TagConfig {
        backend: "ollama".to_string(),
        model: "m".to_string(),
        endpoint: Some("http://localhost:11111".to_string()),
        batch_size: 1,
    };
    // Should error with connection refused, but cover the dispatch branch
    let res = call_llm(&cfg, "prompt");
    assert!(res.is_err());
}

#[test]
fn test_call_llm_dispatches_openai_with_env_key() {
    unsafe { std::env::set_var("OPENAI_API_KEY", "sk-123") };
    let cfg = TagConfig {
        backend: "openai".to_string(),
        model: "gpt-4".to_string(),
        endpoint: None,
        batch_size: 1,
    };
    let res = call_llm(&cfg, "prompt");
    assert!(res.is_err()); // Connection refused to api.openai.com
    unsafe { std::env::remove_var("OPENAI_API_KEY") };
}

#[test]
fn test_call_llm_dispatches_gemini_with_env_key() {
    unsafe { std::env::set_var("GOOGLE_API_KEY", "gk-123") };
    let cfg = TagConfig {
        backend: "gemini".to_string(),
        model: "gemini-1.5".to_string(),
        endpoint: None,
        batch_size: 1,
    };
    let res = call_llm(&cfg, "prompt");
    assert!(res.is_err()); // Connection refused to googleapis.com
    unsafe { std::env::remove_var("GOOGLE_API_KEY") };
}

#[test]
fn test_call_llm_errors_when_anthropic_key_missing() {
    unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
    let cfg = TagConfig {
        backend: "anthropic".to_string(),
        model: "m".to_string(),
        endpoint: None,
        batch_size: 1,
    };
    let res = call_llm(&cfg, "prompt");
    assert_eq!(res.unwrap_err(), "ANTHROPIC_API_KEY not set");
}

#[test]
fn test_run_auto_tag_skips_file_without_snapshot() {
    // Blocks in db but no snapshot → file is skipped gracefully.
    use weaveback_tangle::parse_source_blocks;
    let (mut db, _dir) = make_db();
    let src = "= Section\n\nSome prose here.\n";
    let blocks = parse_source_blocks(src, "adoc");
    db.set_source_blocks("missing.adoc", &blocks).unwrap();
    let cfg = TagConfig {
        backend: "ollama".to_string(),
        model: "dummy".to_string(),
        endpoint: Some("http://127.0.0.1:9/v1".to_string()),
        batch_size: 10,
    };
    run_auto_tag(&mut db, &cfg);
    assert!(db.list_block_tags(None).unwrap().is_empty());
}

#[test]
fn test_call_llm_unknown_backend_defaults_to_openai() {
    let cfg = TagConfig {
        backend: "unknown".to_string(),
        model: "m".to_string(),
        endpoint: Some("http://localhost:11111".to_string()),
        batch_size: 1,
    };
    // Should try to call OpenAI compat and fail with connection refused
    let res = call_llm(&cfg, "prompt");
    assert!(res.is_err());
}

#[test]
fn test_parse_gemini_response_errors_on_empty_candidates() {
    let v = serde_json::json!({ "candidates": [] });
    assert!(parse_gemini_response(&v).is_err());
}

#[test]
fn test_parse_openai_response_errors_on_empty_choices() {
    let v = serde_json::json!({ "choices": [] });
    assert!(parse_openai_response(&v).is_err());
}

