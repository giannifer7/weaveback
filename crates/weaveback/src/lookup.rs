use weaveback_macro::evaluator::output::{PreciseTracingOutput, SourceSpan, SpanKind, SpanRange};
use weaveback_macro::evaluator::{EvalConfig, Evaluator};
use weaveback_macro::macro_api::process_string_precise;
use weaveback_tangle::db::WeavebackDb;
use weaveback_tangle::lookup::{find_line_col, find_best_noweb_entry};
use weaveback_core::PathResolver;
use serde_json::{Value, json};

#[derive(Debug)]
pub enum LookupError {
    Db(weaveback_tangle::db::DbError),
    Io(std::io::Error),
    InvalidInput(String),
}

impl From<weaveback_tangle::db::DbError> for LookupError {
    fn from(e: weaveback_tangle::db::DbError) -> Self {
        LookupError::Db(e)
    }
}

impl From<std::io::Error> for LookupError {
    fn from(e: std::io::Error) -> Self {
        LookupError::Io(e)
    }
}
pub fn perform_where(
    out_file: &str,
    line: u32,
    db: &WeavebackDb,
    resolver: &PathResolver,
) -> Result<Option<Value>, LookupError> {
    if line == 0 {
        return Err(LookupError::InvalidInput("Line number must be >= 1".to_string()));
    }
    let out_line_0 = line - 1;

    if let Some(entry) = find_best_noweb_entry(db, out_file, out_line_0, resolver)? {
        Ok(Some(json!({
            "out_file": out_file,
            "out_line": line,
            "chunk": entry.chunk_name,
            "expanded_file": entry.src_file,
            "expanded_line": entry.src_line + 1,
            "indent": entry.indent,
            "confidence": entry.confidence.as_str(),
        })))
    } else {
        Ok(None)
    }
}
pub fn perform_trace(
    out_file: &str,
    line: u32,
    col: u32,
    db: &WeavebackDb,
    resolver: &PathResolver,
    eval_config: EvalConfig,
) -> Result<Option<Value>, LookupError> {
    if line == 0 {
        return Err(LookupError::InvalidInput("Line number must be >= 1".to_string()));
    }
    let out_line_0 = line - 1;

    let nw_entry = match find_best_noweb_entry(db, out_file, out_line_0, resolver)? {
        None => return Ok(None),
        Some(e) => e,
    };

    let mut result = json!({
        "out_file": out_file,
        "out_line": line,
        "chunk": nw_entry.chunk_name,
        "expanded_file": nw_entry.src_file,
        "expanded_line": nw_entry.src_line + 1,
        "indent": nw_entry.indent,
        "confidence": nw_entry.confidence.as_str(),
    });

    // Re-evaluate the driver file with precise tracing.
    // Prefer the stored snapshot for reproducibility; fall back to disk.
    let src_path = resolver.resolve_src(&nw_entry.src_file);
    let src_content = if let Ok(Some(bytes)) = db.get_src_snapshot(&nw_entry.src_file) {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        match std::fs::read_to_string(&src_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: cannot read {} for trace: {}", nw_entry.src_file, e);
                return Ok(Some(result));
            }
        }
    };

    let mut evaluator = Evaluator::new(eval_config.clone());
    match process_string_precise(&src_content, Some(&src_path), &mut evaluator) {
        Ok((expanded, ranges)) => {
            let expanded_line_0 = nw_entry.src_line;
            // `col` is a 1-indexed character position in the *output* file line,
            // which has `nw_entry.indent` prepended by noweb.  Subtract the
            // indent char count, then convert to 0-indexed before querying the
            // span map.  col=0 is treated as col=1 (default: start of line).
            let indent_char_len = nw_entry.indent.chars().count() as u32;
            let col_1 = col.max(1);
            if col_1 > indent_char_len {
                let adjusted_col_0 = col_1 - 1 - indent_char_len;
                if let Some(span) = span_at_line(&expanded, &ranges, expanded_line_0, adjusted_col_0) {
                    append_span_fields(&mut result, span, &evaluator);
                    let obj = result.as_object_mut().unwrap();
                    match &span.kind {
                        SpanKind::VarBinding { var_name } => {
                            append_def_locations(obj, "set_locations", var_name, db, true);
                        }
                        SpanKind::MacroBody { macro_name } => {
                            append_def_locations(obj, "def_locations", macro_name, db, false);
                        }
                        SpanKind::Computed => {}
                        _ => {}
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Warning: re-evaluation for trace failed: {e}");
        }
    }

    Ok(Some(result))
}
/// Find the `SourceSpan` covering `col_char_0` (0-indexed character position)
/// of 0-indexed `line_0` in the given expanded text and span ranges.
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
        for (i, b) in expanded.bytes().enumerate() {
            if b == b'\n' {
                count += 1;
                if count == line_0 {
                    found = Some(i + 1);
                    break;
                }
            }
        }
        found?
    };
    // Convert 0-indexed char position to byte offset within the line.
    let line_text = &expanded[line_start..];
    let byte_col = line_text
        .char_indices()
        .nth(col_char_0 as usize)
        .map(|(i, _)| i)
        .unwrap_or(line_text.len());
    PreciseTracingOutput::span_at_byte(ranges, line_start + byte_col)
}

/// Append macro-level fields to `result` from `span`.
fn append_span_fields(
    result: &mut Value,
    span: &SourceSpan,
    sources: &Evaluator,
) {
    let src_manager = sources.sources();
    let Some(src_path) = src_manager.source_files().get(span.src as usize) else {
        return;
    };
    let Some(src_bytes) = src_manager.get_source(span.src) else {
        return;
    };
    let src_content = String::from_utf8_lossy(src_bytes);
    let (src_line_1, src_col_1) = find_line_col(&src_content, span.pos);

    let obj = result.as_object_mut().unwrap();
    obj.insert("src_file".into(), Value::String(src_path.to_string_lossy().into_owned()));
    obj.insert("src_line".into(), Value::Number(src_line_1.into()));
    obj.insert("src_col".into(), Value::Number(src_col_1.into()));

    let kind_str = match &span.kind {
        SpanKind::Literal => "Literal",
        SpanKind::MacroBody { .. } => "MacroBody",
        SpanKind::MacroArg { .. } => "MacroArg",
        SpanKind::VarBinding { .. } => "VarBinding",
        SpanKind::Computed => "Computed",
    };
    obj.insert("kind".into(), Value::String(kind_str.to_string()));

    match &span.kind {
        SpanKind::MacroBody { macro_name } => {
            obj.insert("macro_name".into(), Value::String(macro_name.clone()));
        }
        SpanKind::MacroArg { macro_name, param_name } => {
            obj.insert("macro_name".into(), Value::String(macro_name.clone()));
            obj.insert("param_name".into(), Value::String(param_name.clone()));
        }
        SpanKind::VarBinding { var_name } => {
            obj.insert("var_name".into(), Value::String(var_name.clone()));
        }
        _ => {}
    }
}

/// Look up definition sites from the db and append them to `obj` as a JSON array.
/// Each entry has `file`, `line` (1-indexed), and `col` (1-indexed character position).
/// `use_var_defs`: true → query VAR_DEFS, false → query MACRO_DEFS.
fn append_def_locations(
    obj: &mut serde_json::Map<String, Value>,
    field: &str,
    name: &str,
    db: &WeavebackDb,
    use_var_defs: bool,
) {
    let entries = if use_var_defs {
        db.query_var_defs(name)
    } else {
        db.query_macro_defs(name)
    };
    let Ok(entries) = entries else { return };
    if entries.is_empty() { return }
    let locations: Vec<Value> = entries.into_iter().filter_map(|(src_file, pos, _length)| {
        // Resolve position → (line, col) using the stored snapshot.
        let bytes = db.get_src_snapshot(&src_file).ok()??;
        let text = String::from_utf8_lossy(&bytes);
        let (line_1, col_1) = find_line_col(&text, pos as usize);
        Some(json!({
            "file": src_file,
            "line": line_1,
            "col":  col_1,
        }))
    }).collect();
    if !locations.is_empty() {
        obj.insert(field.into(), Value::Array(locations));
    }
}
