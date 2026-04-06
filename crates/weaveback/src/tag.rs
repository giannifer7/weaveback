//! Automatic LLM-based tagging of literate source blocks.
//!
//! Runs as an optional post-step after `weaveback tangle`.  Only blocks
//! whose BLAKE3 content hash has changed since the last run are sent to
//! the LLM; results are cached in `block_tags` and included in the FTS
//! index by `rebuild_prose_fts`.

use weaveback_tangle::db::WeavebackDb;

// ── config ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TagConfig {
    /// Backend name: "anthropic" | "gemini" | "openai" | "ollama"
    pub backend: String,
    /// Model identifier, e.g. "claude-haiku-4-5-20251001".
    pub model: String,
    /// Base URL for openai-compatible / ollama endpoints.
    pub endpoint: Option<String>,
    /// Number of blocks per LLM request (default: 15).
    pub batch_size: usize,
}

// ── synchronous LLM callers ───────────────────────────────────────────────────

fn call_anthropic(api_key: &str, model: &str, prompt: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 512,
        "messages": [{ "role": "user", "content": prompt }]
    });
    let resp = ureq::AgentBuilder::new()
        .build()
        .post("https://api.anthropic.com/v1/messages")
        .set("x-api-key", api_key)
        .set("anthropic-version", "2023-06-01")
        .set("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    v["content"][0]["text"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("unexpected Anthropic response: {v}"))
}

fn call_gemini(api_key: &str, model: &str, prompt: &str) -> Result<String, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );
    let body = serde_json::json!({
        "contents": [{ "role": "user", "parts": [{ "text": prompt }] }],
        "generationConfig": { "maxOutputTokens": 512 }
    });
    let resp = ureq::AgentBuilder::new()
        .build()
        .post(&url)
        .set("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    v["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("unexpected Gemini response: {v}"))
}

fn call_openai_compat(
    api_key: Option<&str>,
    base_url: &str,
    model: &str,
    prompt: &str,
) -> Result<String, String> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "max_tokens": 512
    });
    let mut req = ureq::AgentBuilder::new()
        .build()
        .post(&url)
        .set("content-type", "application/json");
    if let Some(key) = api_key {
        req = req.set("Authorization", &format!("Bearer {key}"));
    }
    let resp = req.send_json(&body).map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("unexpected OpenAI response: {v}"))
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
            let base = cfg
                .endpoint
                .as_deref()
                .unwrap_or("http://localhost:11434/v1");
            call_openai_compat(None, base, &cfg.model, prompt)
        }
        _ => {
            // "openai" or any unknown value → OpenAI-compatible
            let key = std::env::var("OPENAI_API_KEY").ok();
            let base = cfg
                .endpoint
                .as_deref()
                .unwrap_or("https://api.openai.com/v1");
            call_openai_compat(key.as_deref(), base, &cfg.model, prompt)
        }
    }
}

// ── prompt helpers ────────────────────────────────────────────────────────────

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn build_prompt(items: &[(usize, &str, &str)]) -> String {
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

pub(crate) fn parse_response(response: &str) -> Vec<(usize, String)> {
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
            if clean.is_empty() {
                None
            } else {
                Some((idx, clean))
            }
        })
        .collect()
}

pub(crate) fn block_first_line(source: &str, line_start: u32, line_end: u32) -> String {
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

// ── orchestration ─────────────────────────────────────────────────────────────

/// Tag all prose blocks that have no cached tag or whose content changed.
/// Silently skips on API errors (just warns to stderr).
pub fn run_auto_tag(db: &mut WeavebackDb, cfg: &TagConfig) {
    let blocks = match db.get_blocks_needing_tags() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("warning: auto-tag db query failed: {e}");
            return;
        }
    };
    if blocks.is_empty() {
        return;
    }

    // Group blocks by file so we only fetch each snapshot once.
    let mut by_file: std::collections::HashMap<String, Vec<_>> = std::collections::HashMap::new();
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

            for (local_idx, tags) in parse_response(&response) {
                if let Some(b) = chunk.get(local_idx) {
                    if let Err(e) =
                        db.set_block_tags(&b.src_file, b.block_index, &b.content_hash, &tags)
                    {
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
mod tests {
    use super::*;
    use weaveback_tangle::block_parser::SourceBlockEntry;

    // ── parse_response ────────────────────────────────────────────────────────

    #[test]
    fn test_parse_response_basic() {
        let r = parse_response("0:fts,sqlite,search\n1:incremental,hash\n2:tangle");
        assert_eq!(r.len(), 3);
        assert_eq!(r[0], (0, "fts,sqlite,search".to_string()));
        assert_eq!(r[1], (1, "incremental,hash".to_string()));
        assert_eq!(r[2], (2, "tangle".to_string()));
    }

    #[test]
    fn test_parse_response_sanitises_punctuation() {
        // Hyphens kept; spaces and commas between tags work; punctuation stripped.
        let r = parse_response("0:apply-back, safe-write, I/O!");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].1, "apply-back,safe-write,io");
    }

    #[test]
    fn test_parse_response_skips_malformed_lines() {
        let r = parse_response("not a line\n0:good-tag\njunk:also-junk");
        // "junk" does not parse as usize, so only index 0 survives.
        assert_eq!(r.len(), 1);
        assert_eq!(r[0], (0, "good-tag".to_string()));
    }

    #[test]
    fn test_parse_response_empty_tags_skipped() {
        let r = parse_response("0:  ,  ,  ");
        assert!(r.is_empty(), "all-whitespace tags should produce no entry");
    }

    #[test]
    fn test_parse_response_tolerates_extra_colons() {
        // split_once(':') takes only the first colon, rest is tags string.
        let r = parse_response("0:foo:bar,baz");
        assert_eq!(r.len(), 1);
        // "foo:bar" → sanitised → "foobar" (colon stripped), "baz" kept.
        assert_eq!(r[0].1, "foobar,baz");
    }

    // ── build_prompt ──────────────────────────────────────────────────────────

    #[test]
    fn test_build_prompt_contains_all_indices() {
        let items = vec![
            (0usize, "section", "= Introduction"),
            (1, "para", "This is a paragraph."),
        ];
        let p = build_prompt(&items);
        assert!(p.contains("[0] section | = Introduction"));
        assert!(p.contains("[1] para | This is a paragraph."));
    }

    #[test]
    fn test_build_prompt_truncates_long_first_line() {
        let long = "x".repeat(200);
        let items = vec![(0usize, "para", long.as_str())];
        let p = build_prompt(&items);
        // Preview capped at 120 chars.
        let preview: String = "x".repeat(120);
        assert!(p.contains(&preview));
        assert!(!p.contains(&"x".repeat(121)));
    }

    // ── block_first_line ──────────────────────────────────────────────────────

    #[test]
    fn test_block_first_line_normal() {
        let src = "line one\nline two\nline three\n";
        assert_eq!(block_first_line(src, 1, 3), "line one");
        assert_eq!(block_first_line(src, 2, 3), "line two");
    }

    #[test]
    fn test_block_first_line_out_of_range() {
        let src = "only line\n";
        // line_start beyond file length → empty string, no panic.
        assert_eq!(block_first_line(src, 99, 100), "");
    }

    #[test]
    fn test_block_first_line_empty_source() {
        assert_eq!(block_first_line("", 1, 1), "");
    }

    // ── backend selection / orchestration ───────────────────────────────────

    fn block(index: u32, block_type: &str, line_start: u32, line_end: u32) -> SourceBlockEntry {
        SourceBlockEntry {
            block_index: index,
            block_type: block_type.to_string(),
            line_start,
            line_end,
            content_hash: [0u8; 32],
        }
    }

    #[test]
    fn test_call_llm_propagates_backend_errors() {
        let cfg = TagConfig {
            backend: "ollama".to_string(),
            model: "dummy".to_string(),
            endpoint: Some("http://127.0.0.1:9/v1".to_string()),
            batch_size: 1,
        };
        let err = call_llm(&cfg, "prompt").unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn test_run_auto_tag_noop_when_no_blocks_need_tags() {
        let mut db = WeavebackDb::open_temp().unwrap();
        let cfg = TagConfig {
            backend: "ollama".to_string(),
            model: "dummy".to_string(),
            endpoint: Some("http://127.0.0.1:9/v1".to_string()),
            batch_size: 4,
        };
        run_auto_tag(&mut db, &cfg);
        assert!(db.list_block_tags(None).unwrap().is_empty());
    }

    #[test]
    fn test_run_auto_tag_skips_when_snapshot_missing() {
        let mut db = WeavebackDb::open_temp().unwrap();
        db.set_source_blocks(
            "docs/tag.adoc",
            &[block(0, "section", 1, 1), block(1, "para", 3, 3)],
        )
        .unwrap();

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
    fn test_run_auto_tag_skips_when_llm_call_fails() {
        let mut db = WeavebackDb::open_temp().unwrap();
        let source = "= Title\n\nParagraph.\n";
        db.set_src_snapshot("docs/tag.adoc", source.as_bytes()).unwrap();
        db.set_source_blocks(
            "docs/tag.adoc",
            &[block(0, "section", 1, 1), block(1, "para", 3, 3)],
        )
        .unwrap();

        let cfg = TagConfig {
            backend: "ollama".to_string(),
            model: "dummy".to_string(),
            endpoint: Some("http://127.0.0.1:9/v1".to_string()),
            batch_size: 1,
        };
        run_auto_tag(&mut db, &cfg);
        assert!(db.list_block_tags(None).unwrap().is_empty());
    }
}
