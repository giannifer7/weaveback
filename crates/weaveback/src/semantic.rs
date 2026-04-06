use weaveback_tangle::db::WeavebackDb;

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub backend: String,
    pub model: String,
    pub endpoint: Option<String>,
    pub batch_size: usize,
}

pub fn default_embeddings_backend() -> String { "openai".to_string() }
pub fn default_embeddings_model() -> String { "text-embedding-3-small".to_string() }
pub fn default_embeddings_batch_size() -> usize { 24 }

pub fn persist_embedding_config(
    db: &WeavebackDb,
    cfg: &EmbeddingConfig,
) -> Result<(), String> {
    db.set_run_config("semantic.backend", &cfg.backend).map_err(|e| e.to_string())?;
    db.set_run_config("semantic.model", &cfg.model).map_err(|e| e.to_string())?;
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

fn call_openai_embeddings(
    api_key: Option<&str>,
    base_url: &str,
    model: &str,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    let url = format!("{}/embeddings", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "input": inputs,
    });
    let mut req = ureq::AgentBuilder::new()
        .build()
        .post(&url)
        .set("content-type", "application/json");
    if let Some(key) = api_key {
        req = req.set("Authorization", &format!("Bearer {key}"));
    }
    let resp = req.send_json(&body).map_err(|e| e.to_string())?;
    let value: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    let Some(items) = value.get("data").and_then(|v| v.as_array()) else {
        return Err(format!("unexpected embedding response: {value}"));
    };
    items.iter()
        .map(|item| {
            let Some(embedding) = item.get("embedding").and_then(|v| v.as_array()) else {
                return Err(format!("missing embedding array: {item}"));
            };
            embedding.iter()
                .map(|v| v.as_f64().map(|n| n as f32).ok_or_else(|| format!("invalid embedding value: {v}")))
                .collect()
        })
        .collect()
}

fn call_gemini_embeddings(
    api_key: &str,
    model: &str,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:batchEmbedContents?key={}",
        model,
        api_key,
    );
    let requests: Vec<serde_json::Value> = inputs
        .iter()
        .map(|text| {
            serde_json::json!({
                "model": format!("models/{model}"),
                "content": {
                    "parts": [{ "text": text }]
                }
            })
        })
        .collect();
    let body = serde_json::json!({ "requests": requests });
    let resp = ureq::AgentBuilder::new()
        .build()
        .post(&url)
        .set("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| e.to_string())?;
    let value: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    let Some(items) = value.get("embeddings").and_then(|v| v.as_array()) else {
        return Err(format!("unexpected Gemini embedding response: {value}"));
    };
    items.iter()
        .map(|item| {
            let Some(values) = item.get("values").and_then(|v| v.as_array()) else {
                return Err(format!("missing Gemini embedding values: {item}"));
            };
            values.iter()
                .map(|v| v.as_f64().map(|n| n as f32).ok_or_else(|| format!("invalid embedding value: {v}")))
                .collect()
        })
        .collect()
}

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
        "anthropic" => Err("Anthropic does not expose a compatible embedding API here; use tags or an embedding backend".to_string()),
        _ => {
            let key = std::env::var("OPENAI_API_KEY").ok();
            let base = cfg.endpoint.as_deref().unwrap_or("https://api.openai.com/v1");
            call_openai_embeddings(key.as_deref(), base, &cfg.model, inputs)
        }
    }
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
    if blocks.is_empty() {
        return;
    }

    let mut by_file: std::collections::HashMap<String, Vec<_>> = std::collections::HashMap::new();
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
        let lines: Vec<&str> = snapshot.lines().collect();
        for chunk in file_blocks.chunks(batch_size) {
            let texts: Vec<String> = chunk
                .iter()
                .map(|block| {
                    let lo = (block.line_start as usize).saturating_sub(1);
                    let hi = (block.line_end as usize).min(lines.len());
                    if lo >= hi {
                        String::new()
                    } else {
                        lines[lo..hi].join("\n")
                    }
                })
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
    use weaveback_tangle::block_parser::SourceBlockEntry;

    fn hash_bytes(text: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (idx, byte) in text.as_bytes().iter().enumerate() {
            out[idx % out.len()] ^= *byte;
        }
        out
    }

    fn sample_cfg() -> EmbeddingConfig {
        EmbeddingConfig {
            backend: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            endpoint: Some("http://127.0.0.1:9/v1".to_string()),
            batch_size: 2,
        }
    }

    #[test]
    fn defaults_and_persisted_config_roundtrip() {
        let db = WeavebackDb::open_temp().expect("db");
        let cfg = EmbeddingConfig {
            backend: default_embeddings_backend(),
            model: default_embeddings_model(),
            endpoint: Some("http://example.invalid/v1".to_string()),
            batch_size: default_embeddings_batch_size(),
        };

        assert_eq!(cfg.backend, "openai");
        assert_eq!(cfg.model, "text-embedding-3-small");
        assert_eq!(cfg.batch_size, 24);

        persist_embedding_config(&db, &cfg).expect("persist");
        assert_eq!(
            db.get_run_config("semantic.backend").expect("backend").as_deref(),
            Some("openai")
        );
        assert_eq!(
            db.get_run_config("semantic.model").expect("model").as_deref(),
            Some("text-embedding-3-small")
        );
        assert_eq!(
            db.get_run_config("semantic.batch_size").expect("batch").as_deref(),
            Some("24")
        );
        assert_eq!(
            db.get_run_config("semantic.endpoint").expect("endpoint").as_deref(),
            Some("http://example.invalid/v1")
        );
    }

    #[test]
    fn embed_texts_reports_backend_specific_failures() {
        let anthropic = EmbeddingConfig {
            backend: "anthropic".to_string(),
            model: "ignored".to_string(),
            endpoint: None,
            batch_size: 1,
        };
        let gemini = EmbeddingConfig {
            backend: "gemini".to_string(),
            model: "text-embedding-004".to_string(),
            endpoint: None,
            batch_size: 1,
        };
        let ollama = sample_cfg();

        let anthropic_err = embed_texts(&anthropic, &[String::from("hello")]).expect_err("anthropic");
        assert!(anthropic_err.contains("Anthropic"));

        let gemini_err = embed_texts(&gemini, &[String::from("hello")]).expect_err("gemini");
        assert!(!gemini_err.is_empty());

        let ollama_err = embed_texts(&ollama, &[String::from("hello")]).expect_err("ollama");
        assert!(!ollama_err.is_empty());
    }

    #[test]
    fn run_auto_embed_noops_when_no_blocks_need_embeddings() {
        let mut db = WeavebackDb::open_temp().expect("db");
        run_auto_embed(&mut db, &sample_cfg());

        assert_eq!(
            db.get_run_config("semantic.backend").expect("backend").as_deref(),
            Some("ollama")
        );
        assert!(db
            .get_blocks_needing_embeddings("nomic-embed-text")
            .expect("blocks")
            .is_empty());
    }

    #[test]
    fn run_auto_embed_skips_when_snapshot_missing_or_request_fails() {
        let mut db = WeavebackDb::open_temp().expect("db");
        db.set_source_blocks(
            "src/doc.adoc",
            &[SourceBlockEntry {
                block_index: 0,
                block_type: "para".to_string(),
                line_start: 1,
                line_end: 2,
                content_hash: hash_bytes("hello"),
            }],
        )
        .expect("source blocks");

        run_auto_embed(&mut db, &sample_cfg());
        assert_eq!(
            db.get_blocks_needing_embeddings("nomic-embed-text")
                .expect("still needs embedding")
                .len(),
            1
        );

        db.set_src_snapshot("src/doc.adoc", b"hello\nworld\n")
            .expect("snapshot");
        run_auto_embed(&mut db, &sample_cfg());
        assert_eq!(
            db.get_blocks_needing_embeddings("nomic-embed-text")
                .expect("still needs embedding after failed request")
                .len(),
            1
        );
    }
}
