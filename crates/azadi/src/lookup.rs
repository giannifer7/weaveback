use azadi_macros::evaluator::output::{PreciseTracingOutput, SourceSpan, SpanKind, SpanRange};
use azadi_macros::evaluator::{EvalConfig, Evaluator};
use azadi_macros::macro_api::process_string_precise;
use azadi_noweb::db::AzadiDb;
use regex::Regex;
use serde_json::{Value, json};
use std::path::Path;

#[derive(Debug)]
pub enum LookupError {
    Db(azadi_noweb::db::DbError),
    Io(std::io::Error),
    InvalidInput(String),
}

impl From<azadi_noweb::db::DbError> for LookupError {
    fn from(e: azadi_noweb::db::DbError) -> Self {
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
    db: &AzadiDb,
    gen_dir: &Path,
) -> Result<Option<Value>, LookupError> {
    if line == 0 {
        return Err(LookupError::InvalidInput("Line number must be >= 1".to_string()));
    }
    let out_line_0 = line - 1;
    let db_lookup_path = normalize_path(out_file, gen_dir);

    if let Some(entry) = db.get_noweb_entry(&db_lookup_path, out_line_0)? {
        Ok(Some(json!({
            "out_file": out_file,
            "out_line": line,
            "chunk": entry.chunk_name,
            "expanded_file": entry.src_file,
            "expanded_line": entry.src_line + 1,
            "indent": entry.indent,
        })))
    } else {
        Ok(None)
    }
}

/// Re-evaluate the driver file that produced `out_file:line` and return
/// exact token-level attribution by querying the span at the relevant
/// expanded line.
pub fn perform_trace(
    out_file: &str,
    line: u32,
    col: u32,
    db: &AzadiDb,
    gen_dir: &Path,
    eval_config: EvalConfig,
) -> Result<Option<Value>, LookupError> {
    if line == 0 {
        return Err(LookupError::InvalidInput("Line number must be >= 1".to_string()));
    }
    let out_line_0 = line - 1;
    let db_lookup_path = normalize_path(out_file, gen_dir);

    let nw_entry = match db.get_noweb_entry(&db_lookup_path, out_line_0)? {
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
    });

    // Re-evaluate the driver file with precise tracing.
    // Prefer the stored snapshot for reproducibility; fall back to disk.
    let src_path = std::path::Path::new(&nw_entry.src_file);
    let src_content = if let Ok(Some(bytes)) = db.get_src_snapshot(&nw_entry.src_file) {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        match std::fs::read_to_string(src_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: cannot read {} for trace: {}", nw_entry.src_file, e);
                return Ok(Some(result));
            }
        }
    };

    let mut evaluator = Evaluator::new(eval_config.clone());
    match process_string_precise(&src_content, Some(src_path), &mut evaluator) {
        Ok((expanded, ranges)) => {
            let expanded_line_0 = nw_entry.src_line;
            // `col` is the byte offset in the *output* file line, which has
            // `nw_entry.indent` prepended by noweb.  The expanded text has no
            // such indent, so subtract it before querying the span map.
            let indent_len = nw_entry.indent.len() as u32;
            if col >= indent_len {
                let adjusted_col = col - indent_len;
                if let Some(span) = span_at_line(&expanded, &ranges, expanded_line_0, adjusted_col) {
                    append_span_fields(&mut result, span, &evaluator);
                    // For VarBinding: locate the %set(var_name, ...) definition.
                    // For Computed:   locate the %def/%rhaidef/%pydef definition.
                    let obj = result.as_object_mut().unwrap();
                    let special = eval_config.special_char;
                    match &span.kind {
                        SpanKind::VarBinding { var_name } => {
                            if let Some((f, l)) = find_set_definition(var_name, special, &db) {
                                obj.insert("set_file".into(), Value::String(f));
                                obj.insert("set_line".into(), Value::Number(l.into()));
                            }
                        }
                        SpanKind::MacroBody { macro_name } => {
                            if let Some((f, l)) = find_macro_def(macro_name, special, &db) {
                                obj.insert("def_file".into(), Value::String(f));
                                obj.insert("def_line".into(), Value::Number(l.into()));
                            }
                        }
                        SpanKind::Computed => {
                            // macro_name not available for Computed; def_file/def_line
                            // can only be found if the caller already has the name.
                        }
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

/// Find the `SourceSpan` covering the first byte of 0-indexed `line_0`
/// in the given expanded text and span ranges.
fn span_at_line<'a>(
    expanded: &str,
    ranges: &'a [SpanRange],
    line_0: u32,
    col: u32,
) -> Option<&'a SourceSpan> {
    let byte_offset = if line_0 == 0 {
        col as usize
    } else {
        let mut count = 0u32;
        let mut found = None;
        for (i, b) in expanded.bytes().enumerate() {
            if b == b'\n' {
                count += 1;
                if count == line_0 {
                    found = Some(i + 1 + col as usize);
                    break;
                }
            }
        }
        found?
    };
    PreciseTracingOutput::span_at_byte(ranges, byte_offset)
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
    let (src_line, src_col) = find_line_col_0_indexed(&src_content, span.pos);

    let obj = result.as_object_mut().unwrap();
    obj.insert("src_file".into(), Value::String(src_path.to_string_lossy().into_owned()));
    obj.insert("src_line".into(), Value::Number((src_line + 1).into()));
    obj.insert("src_col".into(), Value::Number(src_col.into()));

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

/// Search all source snapshots for the first `%set(var_name, ...)` definition.
/// Returns `(file_path, 1-indexed line)` on the first match.
pub fn find_set_definition(
    var_name: &str,
    special_char: char,
    db: &AzadiDb,
) -> Option<(String, u32)> {
    let pat = format!(
        r"{}set\s*\(\s*{}\s*,",
        regex::escape(&special_char.to_string()),
        regex::escape(var_name),
    );
    let re = Regex::new(&pat).ok()?;
    let snapshots = db.list_src_snapshots().ok()?;
    for (path, bytes) in snapshots {
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.lines().enumerate() {
            if re.is_match(line) {
                return Some((path, i as u32 + 1));
            }
        }
    }
    None
}

/// Search all source snapshots for the first `%rhaidef(name, ...)` or
/// `%pydef(name, ...)` definition.
/// Returns `(file_path, 1-indexed line)` on the first match.
pub fn find_macro_def(
    macro_name: &str,
    special_char: char,
    db: &AzadiDb,
) -> Option<(String, u32)> {
    let s = regex::escape(&special_char.to_string());
    let n = regex::escape(macro_name);
    // Match %def, %rhaidef, %pydef — any flavour
    let pat = format!(r"{}(?:rhai|py)?def\s*\(\s*{}\s*,", s, n);
    let re = Regex::new(&pat).ok()?;
    let snapshots = db.list_src_snapshots().ok()?;
    for (path, bytes) in snapshots {
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.lines().enumerate() {
            if re.is_match(line) {
                return Some((path, i as u32 + 1));
            }
        }
    }
    None
}

fn find_line_col_0_indexed(text: &str, byte_offset: usize) -> (u32, u32) {
    let offset = byte_offset.min(text.len());
    let prefix = &text[..offset];
    let newlines = prefix.bytes().filter(|&b| b == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = (offset - line_start) as u32;
    (newlines, col)
}

pub fn normalize_path_pub(out_file: &str, gen_dir: &Path) -> String {
    normalize_path(out_file, gen_dir)
}

fn normalize_path(out_file: &str, gen_dir: &Path) -> String {
    let mut db_lookup_path = out_file.to_string();
    if let (Ok(canon_gen), Ok(canon_out)) = (
        gen_dir.canonicalize(),
        Path::new(out_file).canonicalize(),
    ) {
        if let Ok(rel) = canon_out.strip_prefix(&canon_gen) {
            db_lookup_path = rel.to_string_lossy().into_owned();
        }
    } else {
        let prefix = gen_dir.to_string_lossy();
        if out_file.starts_with(prefix.as_ref()) {
            let stripped = out_file.trim_start_matches(prefix.as_ref());
            db_lookup_path = stripped.trim_start_matches('/').to_string();
        }
    }
    db_lookup_path
}
