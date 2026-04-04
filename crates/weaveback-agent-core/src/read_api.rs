use crate::workspace::WorkspaceConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use weaveback_core::PathResolver;
use weaveback_macro::evaluator::output::{PreciseTracingOutput, SourceSpan, SpanKind, SpanRange};
use weaveback_macro::evaluator::{EvalConfig, Evaluator};
use weaveback_macro::macro_api::process_string_precise;
use weaveback_tangle::db::WeavebackDb;
use weaveback_tangle::lookup::{find_best_noweb_entry, find_line_col};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub src_file: String,
    pub block_type: String,
    pub line_start: usize,
    pub line_end: usize,
    pub snippet: String,
    pub tags: Vec<String>,
    pub score: f64,
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResult {
    pub out_file: String,
    pub out_line: u32,
    pub chunk: Option<String>,
    pub expanded_file: Option<String>,
    pub expanded_line: Option<u32>,
    pub indent: Option<String>,
    pub confidence: Option<String>,
    pub src_file: Option<String>,
    pub src_line: Option<u32>,
    pub src_col: Option<u32>,
    pub kind: Option<String>,
    pub macro_name: Option<String>,
    pub param_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkContext {
    pub file: String,
    pub name: String,
    pub nth: u32,
    pub def_start: u32,
    pub def_end: u32,
    pub section_breadcrumb: Vec<String>,
    pub prose: String,
    pub body: String,
    pub direct_dependencies: Vec<String>,
    pub dependency_bodies: BTreeMap<String, DependencyBody>,
    pub reverse_dependencies: Vec<String>,
    pub outputs: Vec<String>,
    pub git_log: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyBody {
    pub file: String,
    pub body: String,
}

fn open_db(config: &WorkspaceConfig) -> Result<WeavebackDb, String> {
    if !config.db_path.exists() {
        return Err(format!(
            "Database not found at {}. Run weaveback on your source files first.",
            config.db_path.display()
        ));
    }
    WeavebackDb::open_read_only(&config.db_path).map_err(|e| e.to_string())
}

fn build_eval_config() -> EvalConfig {
    EvalConfig::default()
}

fn span_at_line<'a>(
    expanded: &str,
    ranges: &'a [SpanRange],
    line_0: u32,
    col_char_0: u32,
) -> Option<&'a SourceSpan> {
    let line_start = if line_0 == 0 {
        0usize
    } else {
        let mut count = 0u32;
        let mut found = None;
        for (idx, byte) in expanded.bytes().enumerate() {
            if byte == b'\n' {
                count += 1;
                if count == line_0 {
                    found = Some(idx + 1);
                    break;
                }
            }
        }
        found?
    };

    let line_text = &expanded[line_start..];
    let byte_col = line_text
        .char_indices()
        .nth(col_char_0 as usize)
        .map(|(idx, _)| idx)
        .unwrap_or(line_text.len());

    PreciseTracingOutput::span_at_byte(ranges, line_start + byte_col)
}

fn trace_result_from_span(result: &mut TraceResult, span: &SourceSpan, evaluator: &Evaluator) {
    let source_manager = evaluator.sources();
    let Some(src_path) = source_manager.source_files().get(span.src as usize) else {
        return;
    };
    let Some(src_bytes) = source_manager.get_source(span.src) else {
        return;
    };
    let src_content = String::from_utf8_lossy(src_bytes);
    let (src_line, src_col) = find_line_col(&src_content, span.pos);

    result.src_file = Some(src_path.to_string_lossy().into_owned());
    result.src_line = Some(src_line);
    result.src_col = Some(src_col);
    result.kind = Some(match &span.kind {
        SpanKind::Literal => "Literal",
        SpanKind::MacroBody { .. } => "MacroBody",
        SpanKind::MacroArg { .. } => "MacroArg",
        SpanKind::VarBinding { .. } => "VarBinding",
        SpanKind::Computed => "Computed",
    }
    .to_string());

    match &span.kind {
        SpanKind::MacroBody { macro_name } => {
            result.macro_name = Some(macro_name.clone());
        }
        SpanKind::MacroArg { macro_name, param_name } => {
            result.macro_name = Some(macro_name.clone());
            result.param_name = Some(param_name.clone());
        }
        SpanKind::VarBinding { .. } | SpanKind::Literal | SpanKind::Computed => {}
    }
}

fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    let count = trimmed.bytes().take_while(|&b| b == b'=').count();
    if count > 0 && trimmed.len() > count && trimmed.as_bytes()[count] == b' ' {
        Some(count)
    } else {
        None
    }
}

fn section_range(lines: &[&str], def_start: usize) -> (usize, usize) {
    let mut sec_start = 0usize;
    let mut sec_level = 1usize;
    for idx in (0..def_start).rev() {
        if let Some(level) = heading_level(lines[idx]) {
            sec_start = idx;
            sec_level = level;
            break;
        }
    }

    let sec_end = lines[def_start..]
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| heading_level(line).is_some_and(|level| level <= sec_level))
        .map(|(idx, _)| def_start + idx)
        .unwrap_or(lines.len());

    (sec_start, sec_end)
}

fn title_chain(lines: &[&str], def_start: usize) -> Vec<String> {
    let mut chain = Vec::new();
    for line in lines.iter().take(def_start) {
        if let Some(level) = heading_level(line) {
            let title = line[level + 1..].trim().to_string();
            chain.retain(|(existing_level, _): &(usize, String)| *existing_level < level);
            chain.push((level, title));
        }
    }
    chain.into_iter().map(|(_, title)| title).collect()
}

fn extract_prose(lines: &[&str], start: usize, end: usize) -> String {
    let end = end.min(lines.len());
    let mut in_fence = false;
    let mut out = Vec::new();

    for line in lines.iter().take(end).skip(start) {
        if line.trim() == "----" {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            out.push(*line);
        }
    }

    while out.first().is_some_and(|line| line.trim().is_empty()) {
        out.remove(0);
    }
    while out.last().is_some_and(|line| line.trim().is_empty()) {
        out.pop();
    }

    out.join("\n")
}

fn prepare_fts_query(query: &str) -> String {
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

fn reciprocal_rank(rank: usize) -> f64 {
    1.0 / (60.0 + rank as f64)
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

fn embed_query(db: &WeavebackDb, query: &str) -> Result<Option<Vec<f32>>, String> {
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

    let mut merged: BTreeMap<(String, String, usize, usize), SearchHit> = BTreeMap::new();

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

pub fn trace(
    config: &WorkspaceConfig,
    out_file: &str,
    out_line: u32,
    out_col: u32,
) -> Result<Option<TraceResult>, String> {
    if out_line == 0 {
        return Err("out_line must be >= 1".to_string());
    }

    let db = open_db(config)?;
    let resolver = PathResolver::new(config.project_root.clone(), config.gen_dir.clone());
    let eval_config = build_eval_config();
    let out_line_0 = out_line - 1;
    let nw_entry = match find_best_noweb_entry(&db, out_file, out_line_0, &resolver)
        .map_err(|e| e.to_string())?
    {
        Some(entry) => entry,
        None => return Ok(None),
    };

    let mut result = TraceResult {
        out_file: out_file.to_string(),
        out_line,
        chunk: Some(nw_entry.chunk_name.clone()),
        expanded_file: Some(nw_entry.src_file.clone()),
        expanded_line: Some(nw_entry.src_line + 1),
        indent: Some(nw_entry.indent.clone()),
        confidence: Some(nw_entry.confidence.as_str().to_string()),
        src_file: None,
        src_line: None,
        src_col: None,
        kind: None,
        macro_name: None,
        param_name: None,
    };

    let src_path = resolver.resolve_src(&nw_entry.src_file);
    let src_content = if let Ok(Some(bytes)) = db.get_src_snapshot(&nw_entry.src_file) {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        match std::fs::read_to_string(&src_path) {
            Ok(content) => content,
            Err(_) => return Ok(Some(result)),
        }
    };

    let mut evaluator = Evaluator::new(eval_config);
    if let Ok((expanded, ranges)) = process_string_precise(&src_content, Some(&src_path), &mut evaluator) {
        let indent_char_len = nw_entry.indent.chars().count() as u32;
        let col_1 = out_col.max(1);
        if col_1 > indent_char_len {
            let adjusted_col_0 = col_1 - 1 - indent_char_len;
            if let Some(span) = span_at_line(&expanded, &ranges, nw_entry.src_line, adjusted_col_0) {
                trace_result_from_span(&mut result, span, &evaluator);
            }
        }
    }

    Ok(Some(result))
}

pub fn chunk_context(
    config: &WorkspaceConfig,
    file: &str,
    name: &str,
    nth: u32,
) -> Result<ChunkContext, String> {
    let db = open_db(config)?;
    let entry = db
        .get_chunk_def(file, name, nth)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chunk not found: {file}#{name}[{nth}]"))?;

    let src_path = config.project_root.join(file);
    let src_text = std::fs::read_to_string(&src_path)
        .map_err(|e| format!("Cannot read {}: {e}", src_path.display()))?;
    let src_lines: Vec<&str> = src_text.lines().collect();
    let def_start = entry.def_start as usize;
    let def_end = entry.def_end as usize;

    let body = if def_start < src_lines.len() && def_end <= src_lines.len() && def_end > 0 {
        src_lines[def_start..def_end - 1].join("\n")
    } else {
        String::new()
    };

    let section_breadcrumb = title_chain(&src_lines, def_start);
    let (sec_start, sec_end) = section_range(&src_lines, def_start);
    let prose = extract_prose(&src_lines, sec_start, sec_end);
    let raw_deps = db.query_chunk_deps(name).map_err(|e| e.to_string())?;
    let direct_dependencies = raw_deps.into_iter().map(|(name, _)| name).collect();
    let dep_names = db.query_chunk_deps(name).map_err(|e| e.to_string())?;
    let mut dependency_bodies = BTreeMap::new();
    for (dep_name, _) in &dep_names {
        let defs = db
            .find_chunk_defs_by_name(dep_name)
            .map_err(|e| e.to_string())?;
        let Some(def) = defs.first() else {
            continue;
        };
        let dep_path = config.project_root.join(&def.src_file);
        let dep_text = match std::fs::read_to_string(&dep_path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let dep_lines: Vec<&str> = dep_text.lines().collect();
        let s = def.def_start as usize;
        let e = def.def_end as usize;
        let body = if s < dep_lines.len() && e <= dep_lines.len() && e > 0 {
            dep_lines[s..e - 1].join("\n")
        } else {
            String::new()
        };
        dependency_bodies.insert(
            dep_name.clone(),
            DependencyBody {
                file: def.src_file.clone(),
                body,
            },
        );
    }
    let reverse_dependencies = db
        .query_reverse_deps(name)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(from, _)| from)
        .collect();
    let outputs = db.query_chunk_output_files(name).map_err(|e| e.to_string())?;
    let git_log = match std::process::Command::new("git")
        .args([
            "-C",
            &config.project_root.to_string_lossy(),
            "log",
            "--follow",
            "-n",
            "5",
            "--oneline",
            "--",
            file,
        ])
        .output()
    {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };

    Ok(ChunkContext {
        file: file.to_string(),
        name: name.to_string(),
        nth,
        def_start: entry.def_start,
        def_end: entry.def_end,
        section_breadcrumb,
        prose,
        body,
        direct_dependencies,
        dependency_bodies,
        reverse_dependencies,
        outputs,
        git_log,
    })
}
