// crates/azadi-macros/src/evaluator/output.rs

/// Indicates how a piece of output relates to the original source.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SpanKind {
    /// Literal text from the source document or a textual block.
    Literal,
    /// Text substituted from expanding a macro body.
    MacroBody {
        macro_name: String,
    },
    /// Text substituted from an argument value at a macro call site.
    MacroArg {
        macro_name: String,
        param_name: String,
    },
    /// Text substituted from a global setting or without macro context.
    VarBinding {
        var_name: String,
    },
    /// Text generated programmatically (e.g. Rhai/Python script results, builtins)
    /// that has no direct corresponding source token for its content.
    Computed,
}

/// Byte-offset span referencing the source token that produced a piece of output.
///
/// Fields mirror `Token.src`, `Token.pos`, `Token.length` — no conversion needed.
/// Line/col can be derived on demand by scanning `source[..pos]` for `\n`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceSpan {
    /// Source file index (same as `Token.src`).
    pub src: u32,
    /// Byte offset in the source string (same as `Token.pos`).
    pub pos: usize,
    /// Byte length of the span (same as `Token.length`).
    pub length: usize,
    /// The kind of expansion that produced this text.
    pub kind: SpanKind,
}

/// Generic output sink for the evaluator.
///
/// The evaluator calls `push_str` for every piece of text it produces,
/// providing the `SourceSpan` of the token that generated it.
/// `push_untracked` is used for text whose origin cannot be attributed to
/// a single source span (e.g. Rhai/Python script results).
pub trait EvalOutput {
    /// Append `text` that originated at `span` in the source.
    fn push_str(&mut self, text: &str, span: SourceSpan);

    /// Append text with no span information (computed/script results).
    fn push_untracked(&mut self, text: &str);

    /// Consume the accumulator and return the rendered string.
    fn finish(self) -> String;

    /// Returns `true` for `PreciseTracingOutput`.
    /// Used to opt into per-argument span threading in `evaluate_macro_call_to`.
    fn is_tracing(&self) -> bool {
        false
    }
}

/// Fast-path output accumulator — ignores span info, just collects text.
///
/// This is functionally identical to the existing `String`-based output in
/// `Evaluator::evaluate()`.  Zero overhead: span arguments are discarded.
#[derive(Debug)]
pub struct PlainOutput {
    buf: String,
}

impl PlainOutput {
    pub fn new() -> Self {
        Self {
            buf: String::new(),
        }
    }
}

impl Default for PlainOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl EvalOutput for PlainOutput {
    #[inline]
    fn push_str(&mut self, text: &str, _span: SourceSpan) {
        self.buf.push_str(text);
    }

    #[inline]
    fn push_untracked(&mut self, text: &str) {
        self.buf.push_str(text);
    }

    fn finish(self) -> String {
        self.buf
    }
}

/// Output accumulator that records one source span per output line.
///
/// For each completed output line the first tracked `push_str` span on that
/// line is stored.  Untracked pushes (Rhai results, builtins) advance the line
/// counter but do not contribute a span.
///
/// This is much cheaper than recording per-push-call byte offsets: allocations
/// are proportional to line count rather than token count.
#[derive(Debug)]
pub struct TracingOutput {
    buf: String,
    /// One entry per completed output line (terminated by `\n`).
    /// `None` means that line had no tracked source span.
    line_spans: Vec<Option<SourceSpan>>,
    /// Span for the current, still-open output line.
    current_line_span: Option<SourceSpan>,
}

impl TracingOutput {
    pub fn new() -> Self {
        Self {
            buf: String::new(),
            line_spans: Vec::new(),
            current_line_span: None,
        }
    }

    fn advance_line(&mut self) {
        self.line_spans.push(self.current_line_span.take());
    }
}

impl Default for TracingOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl EvalOutput for TracingOutput {
    fn push_str(&mut self, text: &str, span: SourceSpan) {
        if text.is_empty() {
            return;
        }
        // First tracked span on each line wins.
        if self.current_line_span.is_none() {
            self.current_line_span = Some(span.clone());
        }
        let bytes = text.as_bytes();
        for i in 0..bytes.len() {
            if bytes[i] == b'\n' {
                self.advance_line();
                // Propagate span to the next line only when more content follows
                // this '\n' within the same push_str (intermediate line of a
                // multi-line literal).  If '\n' is the last byte, leave
                // current_line_span as None so the next push_str starts fresh.
                if i + 1 < bytes.len() && self.current_line_span.is_none() {
                    self.current_line_span = Some(span.clone());
                }
            }
        }
        self.buf.push_str(text);
    }

    fn push_untracked(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        // Untracked text advances line boundaries but does not set a span.
        for b in text.bytes() {
            if b == b'\n' {
                self.advance_line();
            }
        }
        self.buf.push_str(text);
    }

    fn finish(self) -> String {
        self.buf
    }
}

/// A serialized entry stored in the `macro_map` database table.
/// It maps an output line (indirectly via the table key) to the original
/// `.md` source file that generated it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MacroMapEntry {
    /// Path of the source (literate) file containing the original text.
    pub src_file: String,
    /// 0-indexed line number within the source file.
    pub src_line: u32,
    /// 0-indexed column (byte offset) within the source line.
    pub src_col: u32,
    /// The kind of macro expansion that produced this text.
    pub kind: SpanKind,
}

use crate::evaluator::state::SourceManager;

impl TracingOutput {
    /// Convert the per-line span records into `MacroMapEntry`s suitable for
    /// storage in the `macro_map` database table.
    ///
    /// Returns a list of `(expanded_line_index, MacroMapEntry)`.
    pub fn into_macro_map_entries(
        &self,
        sources: &SourceManager,
    ) -> Vec<(u32, MacroMapEntry)> {
        // Collect completed lines, plus the final open line if the output does
        // not end with a newline.
        let final_span = if !self.buf.is_empty() && !self.buf.ends_with('\n') {
            Some(self.current_line_span.as_ref())
        } else {
            None
        };

        let base = self.line_spans.iter().map(|s| s.as_ref());
        let all: Box<dyn Iterator<Item = Option<&SourceSpan>>> = match final_span {
            Some(s) => Box::new(base.chain(std::iter::once(s))),
            None => Box::new(base),
        };

        let mut results = Vec::new();
        for (line_idx, maybe_span) in all.enumerate() {
            let Some(span) = maybe_span else { continue };
            let Some(src_path) = sources.source_files().get(span.src as usize) else {
                continue;
            };
            let Some(src_content_bytes) = sources.get_source(span.src) else {
                continue;
            };
            let src_content = String::from_utf8_lossy(src_content_bytes);
            let (src_line, src_col) = find_line_col_0_indexed(&src_content, span.pos);
            results.push((
                line_idx as u32,
                MacroMapEntry {
                    src_file: src_path.to_string_lossy().into_owned(),
                    src_line,
                    src_col,
                    kind: span.kind.clone(),
                },
            ));
        }
        results
    }
}

/// Helper to convert a byte offset into a 0-indexed (line, col)
fn find_line_col_0_indexed(text: &str, byte_offset: usize) -> (u32, u32) {
    let offset = byte_offset.min(text.len());
    let prefix = &text[..offset];
    let newlines = prefix.bytes().filter(|&b| b == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = (offset - line_start) as u32;
    (newlines, col)
}

/// A contiguous byte range in the output attributed to one source token.
/// Gaps (script/builtin results) are absent from the list.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpanRange {
    pub start: usize,
    pub end: usize,
    pub span: SourceSpan,
}

/// Output accumulator with exact per-byte source attribution.
///
/// Records one `SpanRange` entry per source-token transition — far fewer
/// entries than bytes, and no granularity tradeoff.
///
/// Use `into_parts()` to obtain `(output_string, Vec<SpanRange>)`.
/// Use `span_at_byte` to query which span covers a given byte offset.
#[derive(Debug, Default)]
pub struct PreciseTracingOutput {
    buf: String,
    ranges: Vec<SpanRange>,
    current_span: Option<SourceSpan>,
    current_start: usize,
}

impl PreciseTracingOutput {
    pub fn new() -> Self {
        Self::default()
    }

    fn flush_current(&mut self) {
        if let Some(span) = self.current_span.take() {
            self.ranges.push(SpanRange {
                start: self.current_start,
                end: self.buf.len(),
                span,
            });
        }
    }

    /// Consume and return `(output_string, span_ranges)`.
    /// The ranges are sorted by `start` and cover only tracked regions.
    pub fn into_parts(mut self) -> (String, Vec<SpanRange>) {
        self.flush_current();
        (self.buf, self.ranges)
    }

    /// Return the `SourceSpan` covering `byte_offset`, or `None` for untracked gaps.
    pub fn span_at_byte(ranges: &[SpanRange], byte_offset: usize) -> Option<&SourceSpan> {
        let idx = ranges.partition_point(|sr| sr.start <= byte_offset);
        if idx == 0 {
            return None;
        }
        let sr = &ranges[idx - 1];
        if byte_offset < sr.end { Some(&sr.span) } else { None }
    }
}

impl EvalOutput for PreciseTracingOutput {
    fn is_tracing(&self) -> bool {
        true
    }

    fn push_str(&mut self, text: &str, span: SourceSpan) {
        if text.is_empty() {
            return;
        }
        let same = self.current_span.as_ref().map_or(false, |s| {
            s.src == span.src && s.pos == span.pos && s.length == span.length
        });
        if !same {
            self.flush_current();
            self.current_start = self.buf.len();
            self.current_span = Some(span);
        }
        self.buf.push_str(text);
    }

    fn push_untracked(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.flush_current(); // end current span; gap follows
        self.buf.push_str(text);
    }

    fn finish(self) -> String {
        self.into_parts().0
    }
}
