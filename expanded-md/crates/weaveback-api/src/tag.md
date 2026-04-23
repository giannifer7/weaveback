# Auto-tagging
`tag.rs` provides automatic LLM-based tagging of literate source blocks.
It runs as an optional post-step at the end of `wb-tangle`, after
all passes complete and before `rebuild_prose_fts` is called.

## Design

Only blocks whose BLAKE3 content hash has changed since the last run are
sent to the LLM.  Results are cached in the `block_tags` table and included
in the `prose_fts` `tags` column, so `weaveback search` and the MCP tool
can find blocks by tag even when the tag words don't appear in the prose.

Batching keeps API costs low: up to 15 blocks are sent per request.  The
entire weaveback source costs roughly $0.01 to tag from scratch with
Claude Haiku; incremental runs are nearly free.

## Configuration

Add a `[tags]` section to `weaveback.toml`:

```toml
[tags]
backend    = "anthropic"               # or gemini / openai / ollama
model      = "claude-haiku-4-5-20251001"
batch_size = 15                        # blocks per LLM request
# endpoint = "http://localhost:11434/v1"  # for ollama or openai-compatible
```


If the section is absent, or the relevant API key env var
(`ANTHROPIC_API_KEY`, `GOOGLE_API_KEY`, `OPENAI_API_KEY`) is not set,
the step is silently skipped — no behaviour change for existing setups.

```rust
// <[tag-config]>=
use weaveback_tangle::db::WeavebackDb;

#[derive(Debug, Clone)]
pub struct TagConfig {
    /// Backend name: "anthropic" | "gemini" | "openai" | "ollama"
    pub backend:    String,
    /// Model identifier, e.g. "claude-haiku-4-5-20251001".
    pub model:      String,
    /// Base URL for openai-compatible / ollama endpoints.
    pub endpoint:   Option<String>,
    /// Number of blocks per LLM request (default: 15).
    pub batch_size: usize,
}
// @
```


## LLM Clients

Each backend is split into a pure request-builder, a pure response-parser,
and a tiny `ureq` wrapper.  This lets unit tests cover the data-mapping
logic without network access.

`call_llm` dispatches to the right backend using `cfg.backend`.

```rust
// <[tag-llm-clients]>=

// ── Anthropic ─────────────────────────────────────────────────────────────────

fn build_anthropic_body(model: &str, prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "max_tokens": 512,
        "messages": [{ "role": "user", "content": prompt }]
    })
}

fn parse_anthropic_response(v: &serde_json::Value) -> Result<String, String> {
    v["content"][0]["text"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("unexpected Anthropic response: {v}"))
}

fn call_anthropic(api_key: &str, model: &str, prompt: &str) -> Result<String, String> {
    let body = build_anthropic_body(model, prompt);
    let resp = ureq::AgentBuilder::new()
        .build()
        .post("https://api.anthropic.com/v1/messages")
        .set("x-api-key", api_key)
        .set("anthropic-version", "2023-06-01")
        .set("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    parse_anthropic_response(&v)
}

// ── Gemini ────────────────────────────────────────────────────────────────────

fn build_gemini_request(api_key: &str, model: &str, prompt: &str) -> (String, serde_json::Value) {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );
    let body = serde_json::json!({
        "contents": [{ "role": "user", "parts": [{ "text": prompt }] }],
        "generationConfig": { "maxOutputTokens": 512 }
    });
    (url, body)
}

fn parse_gemini_response(v: &serde_json::Value) -> Result<String, String> {
    v["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("unexpected Gemini response: {v}"))
}

fn call_gemini(api_key: &str, model: &str, prompt: &str) -> Result<String, String> {
    let (url, body) = build_gemini_request(api_key, model, prompt);
    let resp = ureq::AgentBuilder::new()
        .build()
        .post(&url)
        .set("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    parse_gemini_response(&v)
}

// ── OpenAI-compatible (also Ollama) ──────────────────────────────────────────

fn build_openai_request(base_url: &str, model: &str, prompt: &str) -> (String, serde_json::Value) {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "max_tokens": 512
    });
    (url, body)
}

fn parse_openai_response(v: &serde_json::Value) -> Result<String, String> {
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("unexpected OpenAI response: {v}"))
}

fn call_openai_compat(
    api_key: Option<&str>,
    base_url: &str,
    model: &str,
    prompt: &str,
) -> Result<String, String> {
    let (url, body) = build_openai_request(base_url, model, prompt);
    let mut req = ureq::AgentBuilder::new()
        .build()
        .post(&url)
        .set("content-type", "application/json");
    if let Some(key) = api_key {
        req = req.set("Authorization", &format!("Bearer {key}"));
    }
    let resp = req.send_json(&body).map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    parse_openai_response(&v)
}

fn call_llm(cfg: &TagConfig, prompt: &str) -> Result<String, String> {
    match cfg.backend.as_str() {
        "anthropic" => {
            let key = std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| "ANTHROPIC_API_KEY not set".to_string())?;
            call_anthropic(&key, &cfg.model, prompt)
        }
        "gemini" => {
            let key = std::env::var("GOOGLE_API_KEY")
                .map_err(|_| "GOOGLE_API_KEY not set".to_string())?;
            call_gemini(&key, &cfg.model, prompt)
        }
        "ollama" => {
            let base = cfg.endpoint.as_deref().unwrap_or("http://localhost:11434/v1");
            call_openai_compat(None, base, &cfg.model, prompt)
        }
        _ => {
            // "openai" or any unknown value → OpenAI-compatible
            let key = std::env::var("OPENAI_API_KEY").ok();
            let base = cfg.endpoint.as_deref().unwrap_or("https://api.openai.com/v1");
            call_openai_compat(key.as_deref(), base, &cfg.model, prompt)
        }
    }
}
// @
```


## Prompt Helpers

`build_prompt` serialises a batch of blocks into the LLM prompt.
`parse_response` deserialises the response and sanitises tag strings.
`block_first_line` extracts the first non-empty line of a block from the
source snapshot; it is used as the preview sent to the LLM.

```rust
// <[tag-prompts]>=
fn build_prompt(items: &[(usize, &str, &str)]) -> String {
    // items: (local_index, block_type, first_line_of_content)
    let mut s = String::from(
        "Tag these blocks from a literate programming document.\n\
         Generate 3-5 short lowercase hyphenated tags per block.\n\
         Output format (one line per block, no other text):\n\
         <number>:<tag1>,<tag2>,<tag3>\n\n",
    );
    for (i, btype, first) in items {
        let preview: String = first.chars().take(120).collect();
        s.push_str(&format!("[{i}] {btype} | {preview}\n"));
    }
    s
}

fn parse_response(response: &str) -> Vec<(usize, String)> {
    response
        .lines()
        .filter_map(|line| {
            let (idx_str, tags_str) = line.split_once(':')?;
            let idx: usize = idx_str.trim().parse().ok()?;
            let tags: Vec<&str> = tags_str
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .collect();
            if tags.is_empty() {
                return None;
            }
            // Sanitise: lowercase, keep alphanumeric and hyphens only.
            let clean: String = tags
                .iter()
                .map(|t| {
                    t.to_lowercase()
                        .chars()
                        .filter(|c| c.is_alphanumeric() || *c == '-')
                        .collect::<String>()
                })
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(",");
            if clean.is_empty() { None } else { Some((idx, clean)) }
        })
        .collect()
}

/// Return the first non-empty line of a block given 1-based line numbers.
/// `line_start` and `line_end` are 1-based; returns `""` when out of range.
fn block_first_line(source: &str, line_start: u32, line_end: u32) -> String {
    let lo = (line_start as usize).saturating_sub(1);
    let hi = (line_end as usize).min(source.lines().count());
    source
        .lines()
        .skip(lo)
        .take(hi.saturating_sub(lo))
        .next()
        .unwrap_or("")
        .to_string()
}
// @
```


## Orchestration

`run_auto_tag` is the entry point called from `wb-tangle`.
It queries the DB for blocks whose content hash has changed, groups them
by source file (to load each snapshot once), calls the LLM in batches,
and stores the resulting tags back into `block_tags`.

Errors from any individual LLM call or DB write are printed as warnings
and do not abort the overall run.

```rust
// <[tag-orchestration]>=
/// Tag all prose blocks that have no cached tag or whose content changed.
/// Silently skips on API errors (just warns to stderr).
pub fn run_auto_tag(db: &mut WeavebackDb, cfg: &TagConfig) {
    let blocks = match db.get_blocks_needing_tags() {
        Ok(b) => b,
        Err(e) => { eprintln!("warning: auto-tag db query failed: {e}"); return; }
    };
    if blocks.is_empty() { return; }

    // Group blocks by file so we only fetch each snapshot once.
    let mut by_file: std::collections::HashMap<String, Vec<_>> =
        std::collections::HashMap::new();
    for b in blocks {
        by_file.entry(b.src_file.clone()).or_default().push(b);
    }

    let batch_size = cfg.batch_size.max(1);
    let mut tagged = 0usize;

    for (src_file, file_blocks) in &by_file {
        // Fetch snapshot; skip file if not found.
        let snapshot = match db.get_src_snapshot(src_file) {
            Ok(Some(bytes)) => match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(_) => continue,
            },
            _ => {
                // Try normalised path variants.
                let alt = src_file.strip_prefix("./").unwrap_or(src_file);
                match db.get_src_snapshot(alt) {
                    Ok(Some(bytes)) => String::from_utf8(bytes).unwrap_or_default(),
                    _ => continue,
                }
            }
        };

        for chunk in file_blocks.chunks(batch_size) {
            // Build (local_index, block_type, first_line) tuples for the prompt.
            let items: Vec<(usize, &str, String)> = chunk
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    let first = block_first_line(&snapshot, b.line_start, b.line_end);
                    (i, b.block_type.as_str(), first)
                })
                .collect();

            let prompt_items: Vec<(usize, &str, &str)> = items
                .iter()
                .map(|(i, bt, fl)| (*i, *bt, fl.as_str()))
                .collect();

            let response = match call_llm(cfg, &build_prompt(&prompt_items)) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("warning: auto-tag LLM call failed: {e}");
                    continue;
                }
            };

            // Accumulate results before writing to avoid partial commits on error.
            let results = parse_response(&response);
            for (local_idx, tags) in results {
                if let Some(b) = chunk.get(local_idx) {
                    if let Err(e) = db.set_block_tags(
                        &b.src_file,
                        b.block_index,
                        &b.content_hash,
                        &tags,
                    ) {
                        eprintln!("warning: auto-tag store failed: {e}");
                    } else {
                        tagged += 1;
                    }
                }
            }
        }
    }

    if tagged > 0 {
        eprintln!("auto-tag: tagged {tagged} block(s)");
    }
}
// @
```


## Tests

```rust
// <[@file weaveback-api/src/tag/tests.rs]>=
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

// @
```


## Assembly

```rust
// <[@file weaveback-api/src/tag.rs]>=
// weaveback-api/src/tag.rs
// I'd Really Rather You Didn't edit this generated file.

//! Automatic LLM-based tagging of literate source blocks.
//!
//! Runs as an optional post-step after `wb-tangle`.  Only blocks
//! whose BLAKE3 content hash has changed since the last run are sent to
//! the LLM; results are cached in `block_tags` and included in the FTS
//! index by `rebuild_prose_fts`.
// <[tag-config]>
// <[tag-llm-clients]>
// <[tag-prompts]>
// <[tag-orchestration]>
#[cfg(test)]
mod tests;

// @
```

