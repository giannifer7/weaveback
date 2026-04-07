//! Lightweight semantic retrieval: optional prose embeddings stored in SQLite.
//!
//! Runs as an optional post-step after `weaveback tangle`.  Only blocks
//! whose BLAKE3 content hash has changed since the last run are sent to
//! the embedding API; results are cached in `block_embeddings` and fused
//! with FTS results at query time.
use weaveback_tangle::db::WeavebackDb;

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub backend:    String,
    pub model:      String,
    pub endpoint:   Option<String>,
    pub batch_size: usize,
}

pub fn default_embeddings_backend()    -> String { "openai".to_string() }
pub fn default_embeddings_model()      -> String { "text-embedding-3-small".to_string() }
pub fn default_embeddings_batch_size() -> usize  { 24 }

pub fn persist_embedding_config(
    db: &WeavebackDb,
    cfg: &EmbeddingConfig,
) -> Result<(), String> {
    db.set_run_config("semantic.backend", &cfg.backend).map_err(|e| e.to_string())?;
    db.set_run_config("semantic.model",   &cfg.model  ).map_err(|e| e.to_string())?;
    db.set_run_config(
        "semantic.batch_size",
        &cfg.batch_size.to_string(),
    ).map_err(|e| e.to_string())?;
    db.set_run_config(
        "semantic.endpoint",
        cfg.endpoint.as_deref().unwrap_or(""),
    ).map_err(|e| e.to_string())?;
    Ok(())
}

// ── OpenAI-compatible (also Ollama) ──────────────────────────────────────────

fn build_openai_embedding_request(
    base_url: &str,
    model: &str,
    inputs: &[String],
) -> (String, serde_json::Value) {
    let url = format!("{}/embeddings", base_url.trim_end_matches('/'));
    let body = serde_json::json!({ "model": model, "input": inputs });
    (url, body)
}

fn parse_openai_embedding_response(
    value: &serde_json::Value,
) -> Result<Vec<Vec<f32>>, String> {
    let Some(items) = value.get("data").and_then(|v| v.as_array()) else {
        return Err(format!("unexpected embedding response: {value}"));
    };
    items.iter()
        .map(|item| {
            let Some(embedding) = item.get("embedding").and_then(|v| v.as_array()) else {
                return Err(format!("missing embedding array: {item}"));
            };
            embedding.iter()
                .map(|v| v.as_f64().map(|n| n as f32)
                    .ok_or_else(|| format!("invalid embedding value: {v}")))
                .collect()
        })
        .collect()
}

fn call_openai_embeddings(
    api_key: Option<&str>,
    base_url: &str,
    model: &str,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    let (url, body) = build_openai_embedding_request(base_url, model, inputs);
    let mut req = ureq::AgentBuilder::new()
        .build()
        .post(&url)
        .set("content-type", "application/json");
    if let Some(key) = api_key {
        req = req.set("Authorization", &format!("Bearer {key}"));
    }
    let resp = req.send_json(&body).map_err(|e| e.to_string())?;
    let value: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    parse_openai_embedding_response(&value)
}

// ── Gemini ────────────────────────────────────────────────────────────────────

fn build_gemini_embedding_request(
    api_key: &str,
    model: &str,
    inputs: &[String],
) -> (String, serde_json::Value) {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:batchEmbedContents?key={}",
        model, api_key,
    );
    let requests: Vec<serde_json::Value> = inputs
        .iter()
        .map(|text| serde_json::json!({
            "model": format!("models/{model}"),
            "content": { "parts": [{ "text": text }] }
        }))
        .collect();
    let body = serde_json::json!({ "requests": requests });
    (url, body)
}

fn parse_gemini_embedding_response(
    value: &serde_json::Value,
) -> Result<Vec<Vec<f32>>, String> {
    let Some(items) = value.get("embeddings").and_then(|v| v.as_array()) else {
        return Err(format!("unexpected Gemini embedding response: {value}"));
    };
    items.iter()
        .map(|item| {
            let Some(values) = item.get("values").and_then(|v| v.as_array()) else {
                return Err(format!("missing Gemini embedding values: {item}"));
            };
            values.iter()
                .map(|v| v.as_f64().map(|n| n as f32)
                    .ok_or_else(|| format!("invalid embedding value: {v}")))
                .collect()
        })
        .collect()
}

fn call_gemini_embeddings(
    api_key: &str,
    model: &str,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    let (url, body) = build_gemini_embedding_request(api_key, model, inputs);
    let resp = ureq::AgentBuilder::new()
        .build()
        .post(&url)
        .set("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| e.to_string())?;
    let value: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    parse_gemini_embedding_response(&value)
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

fn embed_texts(cfg: &EmbeddingConfig, inputs: &[String]) -> Result<Vec<Vec<f32>>, String> {
    match cfg.backend.as_str() {
        "gemini" => {
            let key = std::env::var("GOOGLE_API_KEY")
                .map_err(|_| "GOOGLE_API_KEY not set".to_string())?;
            call_gemini_embeddings(&key, &cfg.model, inputs)
        }
        "ollama" => {
            let base = cfg.endpoint.as_deref().unwrap_or("http://localhost:11434/v1");
            call_openai_embeddings(None, base, &cfg.model, inputs)
        }
        "anthropic" => Err(
            "Anthropic does not expose a compatible embedding API here; \
             use tags or an embedding backend".to_string()
        ),
        _ => {
            let key = std::env::var("OPENAI_API_KEY").ok();
            let base = cfg.endpoint.as_deref().unwrap_or("https://api.openai.com/v1");
            call_openai_embeddings(key.as_deref(), base, &cfg.model, inputs)
        }
    }
}
/// Extract the text of a block (1-based `line_start`..`line_end`) from a
/// snapshot string.  Returns an empty string when the range is out of bounds.
fn extract_block_text(snapshot: &str, line_start: u32, line_end: u32) -> String {
    let lo = (line_start as usize).saturating_sub(1);
    let hi = line_end as usize;
    snapshot
        .lines()
        .skip(lo)
        .take(hi.saturating_sub(lo))
        .collect::<Vec<_>>()
        .join("\n")
}
pub fn run_auto_embed(
    db: &mut WeavebackDb,
    cfg: &EmbeddingConfig,
) {
    if let Err(err) = persist_embedding_config(db, cfg) {
        eprintln!("warning: embedding config store failed: {err}");
    }

    let blocks = match db.get_blocks_needing_embeddings(&cfg.model) {
        Ok(items) => items,
        Err(err) => {
            eprintln!("warning: auto-embed db query failed: {err}");
            return;
        }
    };
    if blocks.is_empty() { return; }

    // Group by file so each snapshot is fetched at most once.
    let mut by_file: std::collections::HashMap<String, Vec<_>> =
        std::collections::HashMap::new();
    for block in blocks {
        by_file.entry(block.src_file.clone()).or_default().push(block);
    }

    let batch_size = cfg.batch_size.max(1);
    let mut embedded = 0usize;

    for (src_file, file_blocks) in by_file {
        let snapshot = match db.get_src_snapshot(&src_file) {
            Ok(Some(bytes)) => String::from_utf8(bytes).unwrap_or_default(),
            _ => {
                let alt = src_file.strip_prefix("./").unwrap_or(src_file.as_str());
                match db.get_src_snapshot(alt) {
                    Ok(Some(bytes)) => String::from_utf8(bytes).unwrap_or_default(),
                    _ => continue,
                }
            }
        };

        for chunk in file_blocks.chunks(batch_size) {
            let texts: Vec<String> = chunk
                .iter()
                .map(|block| extract_block_text(&snapshot, block.line_start, block.line_end))
                .collect();

            let vectors = match embed_texts(cfg, &texts) {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("warning: auto-embed request failed: {err}");
                    continue;
                }
            };
            for (block, vector) in chunk.iter().zip(vectors.iter()) {
                if let Err(err) = db.set_block_embedding(
                    &src_file,
                    block.block_index,
                    &block.content_hash,
                    &cfg.model,
                    vector,
                ) {
                    eprintln!("warning: auto-embed store failed: {err}");
                } else {
                    embedded += 1;
                }
            }
        }
    }
    if embedded > 0 {
        eprintln!("auto-embed: embedded {embedded} block(s)");
    }
}
#[cfg(test)]
mod tests {
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
}
