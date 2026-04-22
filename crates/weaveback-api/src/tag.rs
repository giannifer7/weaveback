// weaveback-api/src/tag.rs
// I'd Really Rather You Didn't edit this generated file.

//! Automatic LLM-based tagging of literate source blocks.
//!
//! Runs as an optional post-step after `wb-tangle`.  Only blocks
//! whose BLAKE3 content hash has changed since the last run are sent to
//! the LLM; results are cached in `block_tags` and included in the FTS
//! index by `rebuild_prose_fts`.
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
#[cfg(test)]
mod tests;

