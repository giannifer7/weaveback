// crates/weaveback-macro/src/evaluator/tests/test_output.rs

use crate::evaluator::output::{EvalOutput, PlainOutput, PreciseTracingOutput, SourceSpan};
use crate::evaluator::{EvalConfig, Evaluator};
use crate::evaluator::state::SourceManager;
use crate::macro_api::process_string_defaults;
use std::path::PathBuf;

/// Helper: evaluate `source` through evaluate_to(PlainOutput) and return the
/// resulting String.
fn eval_to_plain(source: &str) -> String {
    let mut eval = Evaluator::new(EvalConfig::default());
    let path = PathBuf::from("<test>");
    let ast = eval.parse_string(source, &path).unwrap();
    let mut out = PlainOutput::default();
    eval.evaluate_to(&ast, &mut out).unwrap();
    out.finish()
}

// ---------- Parity tests: evaluate_to(PlainOutput) == evaluate() ----------

#[test]
fn plain_output_parity_literal_text() {
    let src = "hello world";
    let via_evaluate = String::from_utf8(process_string_defaults(src).unwrap()).unwrap();
    let via_output = eval_to_plain(src);
    assert_eq!(via_output, via_evaluate);
}

#[test]
fn plain_output_parity_variable() {
    let src = "%set(x, 42)%(x) is the answer";
    let via_evaluate = String::from_utf8(process_string_defaults(src).unwrap()).unwrap();
    let via_output = eval_to_plain(src);
    assert_eq!(via_output, via_evaluate);
}

#[test]
fn plain_output_parity_def_and_call() {
    let src = "%def(greet, name, %{Hello, %(name)!%})%greet(World)";
    let via_evaluate = String::from_utf8(process_string_defaults(src).unwrap()).unwrap();
    let via_output = eval_to_plain(src);
    assert_eq!(via_output, via_evaluate);
}

#[test]
fn plain_output_parity_nested_macros() {
    let src = r#"%def(inner, %{X%})%def(outer, %{[%inner()]%})%outer()"#;
    let via_evaluate = String::from_utf8(process_string_defaults(src).unwrap()).unwrap();
    let via_output = eval_to_plain(src);
    assert_eq!(via_output, via_evaluate);
}

#[test]
fn plain_output_parity_if() {
    let src = "%set(flag, yes)%if(%(flag), true, false)";
    let via_evaluate = String::from_utf8(process_string_defaults(src).unwrap()).unwrap();
    let via_output = eval_to_plain(src);
    assert_eq!(via_output, via_evaluate);
}

#[test]
fn plain_output_parity_multiline() {
    let src = "%def(tag, name, value, %{<%(name)>%(value)</%(name)>%})\n%tag(div, hello)\n";
    let via_evaluate = String::from_utf8(process_string_defaults(src).unwrap()).unwrap();
    let via_output = eval_to_plain(src);
    assert_eq!(via_output, via_evaluate);
}

#[test]
fn plain_output_parity_named_args() {
    let src = "%def(tag, name, value, %{<%(name)>%(value)</%(name)>%})%tag(name=span, value=hi)";
    let via_evaluate = String::from_utf8(process_string_defaults(src).unwrap()).unwrap();
    let via_output = eval_to_plain(src);
    assert_eq!(via_output, via_evaluate);
}

// ---------- Span correctness: SpyOutput to verify spans ----------

/// A test-only EvalOutput that records spans.
struct SpyOutput {
    buf: String,
    spans: Vec<(String, SourceSpan)>,
    untracked: Vec<String>,
}

impl SpyOutput {
    fn new() -> Self {
        Self {
            buf: String::new(),
            spans: Vec::new(),
            untracked: Vec::new(),
        }
    }
}

impl EvalOutput for SpyOutput {
    fn push_str(&mut self, text: &str, span: SourceSpan) {
        self.buf.push_str(text);
        self.spans.push((text.to_string(), span));
    }

    fn push_untracked(&mut self, text: &str) {
        self.buf.push_str(text);
        self.untracked.push(text.to_string());
    }

    fn finish(self) -> String {
        self.buf
    }
}

#[test]
fn spy_output_captures_literal_spans() {
    let src = "abc";
    let mut eval = Evaluator::new(EvalConfig::default());
    let path = PathBuf::from("<test>");
    let ast = eval.parse_string(src, &path).unwrap();
    let mut spy = SpyOutput::new();
    eval.evaluate_to(&ast, &mut spy).unwrap();

    assert_eq!(spy.buf, "abc");
    assert!(!spy.spans.is_empty(), "no spans were recorded");
    let (text, span) = &spy.spans[0];
    assert_eq!(text, "abc");
    assert_eq!(span.pos, 0);
    assert_eq!(span.length, 3);
}

#[test]
fn spy_output_set_variable_lookup_is_untracked() {
    // %set stores a variable with no span; %(x) therefore goes through
    // push_untracked (no origin token to attribute).  Note: %set itself
    // returns "" so it produces no output at all.
    let src = "%set(x, val)%(x)";
    let mut eval = Evaluator::new(EvalConfig::default());
    let path = PathBuf::from("<test>");
    let ast = eval.parse_string(src, &path).unwrap();
    let mut spy = SpyOutput::new();
    eval.evaluate_to(&ast, &mut spy).unwrap();

    assert_eq!(spy.buf, "val");
    let untracked_texts: Vec<&str> = spy.untracked.iter().map(|t| t.as_str()).collect();
    assert!(
        untracked_texts.contains(&"val"),
        "variable expansion without origin span should be untracked, got: {:?}",
        untracked_texts
    );
}

#[test]
fn spy_output_builtin_result_is_tracked_as_computed() {
    // Builtins that return non-empty strings (e.g. %capitalize) must emit
    // push_str with SpanKind::Computed, not push_untracked.
    // This ensures the tracer can attribute the output line to the call site.
    let src = "%capitalize(hello)";
    let mut eval = Evaluator::new(EvalConfig::default());
    let path = PathBuf::from("<test>");
    let ast = eval.parse_string(src, &path).unwrap();
    let mut spy = SpyOutput::new();
    eval.evaluate_to(&ast, &mut spy).unwrap();

    assert_eq!(spy.buf, "Hello");
    assert!(spy.untracked.is_empty(), "builtin result should not be untracked");
    let (text, span) = spy.spans.iter()
        .find(|(t, _)| t == "Hello")
        .expect("tracked span for 'Hello' not found");
    assert_eq!(text, "Hello");
    assert_eq!(span.kind, SpanKind::Computed,
        "builtin result should carry SpanKind::Computed");
}

#[test]
fn spy_output_user_macro_is_tracked() {
    let src = "%def(wrap, x, %{[%(x)]%})%wrap(hi)";
    let mut eval = Evaluator::new(EvalConfig::default());
    let path = PathBuf::from("<test>");
    let ast = eval.parse_string(src, &path).unwrap();
    let mut spy = SpyOutput::new();
    eval.evaluate_to(&ast, &mut spy).unwrap();

    assert_eq!(spy.buf, "[hi]");
    let tracked_texts: Vec<&str> = spy.spans.iter().map(|(t, _)| t.as_str()).collect();
    assert!(
        tracked_texts.contains(&"["),
        "literal '[' in macro body should be tracked"
    );
    assert!(
        tracked_texts.contains(&"]"),
        "literal ']' in macro body should be tracked"
    );
    assert!(
        tracked_texts.contains(&"hi"),
        "argument substitution should be tracked"
    );
}

// ---------- TracingOutput tests ----------

use crate::evaluator::output::SpanKind;
use crate::evaluator::output::TracingOutput;

fn test_span(kind: SpanKind) -> SourceSpan {
    SourceSpan {
        src: 0,
        pos: 0,
        length: 3,
        kind,
    }
}

#[test]
fn tracing_output_single_line_gets_an_entry() {
    let src = "%def(wrap, x, %{[%(x)]%})%wrap(hi)";
    let mut eval = Evaluator::new(EvalConfig::default());
    let path = PathBuf::from("test.md");
    let ast = eval.parse_string(src, &path).unwrap();
    let mut out = TracingOutput::new();
    eval.evaluate_to(&ast, &mut out).unwrap();

    let entries = out.into_macro_map_entries(eval.sources());
    assert_eq!(out.finish(), "[hi]");
    assert_eq!(entries.len(), 1, "expected one line entry, got: {entries:?}");
    let (out_line, entry) = &entries[0];
    assert_eq!(*out_line, 0);
    assert!(
        matches!(&entry.kind, SpanKind::MacroBody { macro_name } if macro_name == "wrap"),
        "expected MacroBody(wrap), got: {:?}", entry.kind
    );
}

#[test]
fn test_macro_map_entries() {
    let src = "line 1\nline 2 with %set(x, val)%(x)\nline 3";
    let mut eval = Evaluator::new(EvalConfig::default());
    let path = PathBuf::from("test.md");
    let ast = eval.parse_string(src, &path).unwrap();
    let mut out = TracingOutput::new();
    eval.evaluate_to(&ast, &mut out).unwrap();

    let entries = out.into_macro_map_entries(eval.sources());

    assert_eq!(entries.len(), 3);

    let (out_line_0, entry_0) = &entries[0];
    assert_eq!(*out_line_0, 0);
    assert!(entry_0.src_file.ends_with("test.md"));
    assert_eq!(entry_0.src_line, 0);
    assert_eq!(entry_0.src_col, 0);
    assert!(matches!(entry_0.kind, SpanKind::Literal));

    let (out_line_1, entry_1) = &entries[1];
    assert_eq!(*out_line_1, 1);
    assert_eq!(entry_1.src_line, 1);

    let (out_line_2, entry_2) = &entries[2];
    assert_eq!(*out_line_2, 2);
    assert_eq!(entry_2.src_line, 2);
}

#[test]
fn tracing_output_multiline_literal_creates_entries_for_each_line() {
    let mut out = TracingOutput::new();
    out.push_str("aa\nbb", test_span(SpanKind::Literal));

    let mut sources = SourceManager::new();
    sources.add_source_bytes(b"aa\nbb".to_vec(), PathBuf::from("src.txt"));
    let entries = out.into_macro_map_entries(&sources);

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, 0);
    assert_eq!(entries[1].0, 1);
    assert_eq!(entries[0].1.src_line, 0);
    assert_eq!(entries[1].1.src_line, 0);
}

#[test]
fn tracing_output_untracked_gap_does_not_create_entry() {
    let mut out = TracingOutput::new();
    out.push_str("a\n", test_span(SpanKind::Literal));
    out.push_untracked("computed\n");
    out.push_str("b", test_span(SpanKind::Literal));

    let mut sources = SourceManager::new();
    sources.add_source_bytes(b"ab".to_vec(), PathBuf::from("src.txt"));
    let entries = out.into_macro_map_entries(&sources);

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, 0);
    assert_eq!(entries[1].0, 2);
}

#[test]
fn precise_tracing_merges_adjacent_equal_spans_and_keeps_gaps() {
    let span = test_span(SpanKind::Literal);
    let mut out = PreciseTracingOutput::new();
    out.push_str("ab", span.clone());
    out.push_str("cd", span.clone());
    out.push_untracked("X");
    out.push_str("ef", span.clone());

    let (buf, ranges) = out.into_parts();
    assert_eq!(buf, "abcdXef");
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0].start, 0);
    assert_eq!(ranges[0].end, 4);
    assert_eq!(ranges[1].start, 5);
    assert_eq!(ranges[1].end, 7);
}

#[test]
fn precise_span_at_byte_handles_hits_and_gaps() {
    let span = test_span(SpanKind::Literal);
    let mut out = PreciseTracingOutput::new();
    out.push_str("abc", span.clone());
    out.push_untracked("X");
    out.push_str("yz", span.clone());
    let (_buf, ranges) = out.into_parts();

    assert!(PreciseTracingOutput::span_at_byte(&ranges, 0).is_some());
    assert!(PreciseTracingOutput::span_at_byte(&ranges, 2).is_some());
    assert!(PreciseTracingOutput::span_at_byte(&ranges, 3).is_none());
    assert!(PreciseTracingOutput::span_at_byte(&ranges, 4).is_some());
    assert!(PreciseTracingOutput::span_at_byte(&ranges, 99).is_none());
}

#[test]
fn tracing_output_into_macro_map_entries_includes_final_open_line() {
    let src = "abc";
    let mut out = TracingOutput::default();
    out.push_str(src, test_span(SpanKind::Literal));

    let mut sm = SourceManager::new();
    sm.add_source_bytes(src.as_bytes().to_vec(), PathBuf::from("inline.txt"));

    let entries = out.into_macro_map_entries(&sm);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, 0);
    assert_eq!(entries[0].1.src_file, "inline.txt");
}

#[test]
fn tracing_output_into_macro_map_entries_skips_invalid_source_ids() {
    let mut out = TracingOutput::new();
    out.push_str(
        "abc",
        SourceSpan {
            src: 99,
            pos: 0,
            length: 3,
            kind: SpanKind::Literal,
        },
    );

    let sm = SourceManager::new();
    let entries = out.into_macro_map_entries(&sm);
    assert!(entries.is_empty());
}

#[test]
fn precise_tracing_output_finish_flushes_pending_range() {
    let mut out = PreciseTracingOutput::new();
    out.push_str("abc", test_span(SpanKind::Literal));
    assert_eq!(out.finish(), "abc");
}
