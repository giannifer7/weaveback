// weaveback-agent-core/src/read_api/trace.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use super::db::{build_eval_config, open_db};

pub(in crate::read_api) fn span_at_line<'a>(
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

pub(in crate::read_api) fn trace_result_from_span(result: &mut TraceResult, span: &SourceSpan, evaluator: &Evaluator) {
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

