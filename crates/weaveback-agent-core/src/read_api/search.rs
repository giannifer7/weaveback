// weaveback-agent-core/src/read_api/search.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use super::db::open_db;

pub(in crate::read_api) fn prepare_fts_query(query: &str) -> String {
    if query.contains('"')
        || query.contains(" AND ")
        || query.contains(" OR ")
        || query.contains(" NOT ")
    {
        return query.to_owned();
    }

    query
        .split_whitespace()
        .map(|token| {
            let safe = token
                .chars()
                .all(|char| char.is_alphanumeric() || char == '*' || char == '^');
            if safe {
                token.to_owned()
            } else {
                format!("\"{}\"", token.replace('"', "\"\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(in crate::read_api) fn reciprocal_rank(rank: usize) -> f64 {
    1.0 / (60.0 + rank as f64)
}

pub(in crate::read_api) fn call_openai_embeddings(
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

pub(in crate::read_api) fn call_gemini_embeddings(
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

pub(in crate::read_api) fn embed_query(db: &WeavebackDb, query: &str) -> Result<Option<Vec<f32>>, String> {
    let Some(model) = db.get_run_config("semantic.model").map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let backend = db
        .get_run_config("semantic.backend")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "openai".to_string());
    let endpoint = db.get_run_config("semantic.endpoint").map_err(|e| e.to_string())?;
    let query_vec = match backend.as_str() {
        "gemini" => {
            let key = std::env::var("GOOGLE_API_KEY")
                .map_err(|_| "GOOGLE_API_KEY not set".to_string())?;
            call_gemini_embeddings(&key, &model, &[query.to_string()])?
        }
        "ollama" => {
            let base = endpoint.as_deref().filter(|v| !v.is_empty()).unwrap_or("http://localhost:11434/v1");
            call_openai_embeddings(None, base, &model, &[query.to_string()])?
        }
        "anthropic" => return Ok(None),
        _ => {
            let key = std::env::var("OPENAI_API_KEY").ok();
            let base = endpoint.as_deref().filter(|v| !v.is_empty()).unwrap_or("https://api.openai.com/v1");
            call_openai_embeddings(key.as_deref(), base, &model, &[query.to_string()])?
        }
    };
    Ok(query_vec.into_iter().next())
}

pub fn search(config: &WorkspaceConfig, query: &str, limit: usize) -> Result<Vec<SearchHit>, String> {
    let db = open_db(config)?;
    let fts_query = prepare_fts_query(query);
    let lexical = db.search_prose(&fts_query, limit.saturating_mul(4)).map_err(|e| e.to_string())?;
    let semantic = embed_query(&db, query)
        .ok()
        .flatten()
        .and_then(|query_embedding| db.search_prose_by_embedding(&query_embedding, limit.saturating_mul(4)).ok())
        .unwrap_or_default();

    let mut merged: std::collections::BTreeMap<(String, String, usize, usize), SearchHit> = std::collections::BTreeMap::new();

    for (idx, result) in lexical.into_iter().enumerate() {
        let key = (
            result.src_file.clone(),
            result.block_type.clone(),
            result.line_start as usize,
            result.line_end as usize,
        );
        let entry = merged.entry(key).or_insert_with(|| SearchHit {
            src_file: result.src_file.clone(),
            block_type: result.block_type.clone(),
            line_start: result.line_start as usize,
            line_end: result.line_end as usize,
            snippet: result.snippet.clone(),
            tags: result
                .tags
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_string)
                .collect(),
            score: 0.0,
            channels: Vec::new(),
        });
        entry.score += reciprocal_rank(idx + 1);
        if !entry.channels.iter().any(|channel| channel == "fts") {
            entry.channels.push("fts".to_string());
        }
    }

    for (idx, result) in semantic.into_iter().enumerate() {
        let key = (
            result.src_file.clone(),
            result.block_type.clone(),
            result.line_start as usize,
            result.line_end as usize,
        );
        let entry = merged.entry(key).or_insert_with(|| SearchHit {
            src_file: result.src_file.clone(),
            block_type: result.block_type.clone(),
            line_start: result.line_start as usize,
            line_end: result.line_end as usize,
            snippet: result.snippet.clone(),
            tags: result
                .tags
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_string)
                .collect(),
            score: 0.0,
            channels: Vec::new(),
        });
        entry.score += reciprocal_rank(idx + 1) + f64::from(result.score.max(0.0)) * 0.25;
        if !entry.channels.iter().any(|channel| channel == "semantic") {
            entry.channels.push("semantic".to_string());
        }
    }

    let mut hits: Vec<SearchHit> = merged.into_values().collect();
    hits.sort_by(|lhs, rhs| {
        rhs.score
            .partial_cmp(&lhs.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| lhs.src_file.cmp(&rhs.src_file))
            .then_with(|| lhs.line_start.cmp(&rhs.line_start))
    });
    hits.truncate(limit);
    Ok(hits)
}

