# Apply-Back Test Primitives

Pure helper behavior and small local transformations used by apply-back.

```rust
// <[applyback-tests-primitives]>=
use super::*;
use std::fs;
use weaveback_tangle::db::{ChunkDefEntry, Confidence};

fn lines(s: &str) -> Vec<String> {
    s.lines().map(str::to_string).collect()
}

// ── fuzzy_find_line ────────────────────────────────────────────────────

#[test]
fn fuzzy_find_line_finds_unique_match() {
    let ls = lines("foo\nbar baz\nqux");
    assert_eq!(fuzzy_find_line(&ls, 1, "bar baz", 5), Some(1));
}

#[test]
fn fuzzy_find_line_returns_none_when_ambiguous() {
    let ls = lines("foo\nfoo\nfoo");
    assert_eq!(fuzzy_find_line(&ls, 1, "foo", 5), None);
}

#[test]
fn fuzzy_find_line_returns_none_outside_window() {
    let ls = lines("match\nother\nother\nother\nother\nother\nother\nother\nother\nother");
    // center=9, window=0 — "match" is at index 0, distance 9 > window 0
    assert_eq!(fuzzy_find_line(&ls, 9, "match", 0), None);
}

#[test]
fn fuzzy_find_line_tolerates_leading_whitespace() {
    // The pattern is anchored with ^\s* and \s*$, so leading/trailing
    // spaces in the source line are ignored.
    let ls = lines("   bar baz   ");
    assert_eq!(fuzzy_find_line(&ls, 0, "bar baz", 0), Some(0));
}

// ── splice_line ────────────────────────────────────────────────────────

#[test]
fn splice_line_replaces_indexed_line() {
    let ls = lines("aaa\nbbb\nccc");
    let result = splice_line(&ls, 1, "BBB", false);
    assert_eq!(result, "aaa\nBBB\nccc");
}

#[test]
fn splice_line_preserves_trailing_newline() {
    let ls = lines("x\ny");
    let result = splice_line(&ls, 0, "X", true);
    assert!(result.ends_with('\n'));
}

// ── token_overlap_score ────────────────────────────────────────────────

#[test]
fn token_overlap_score_counts_shared_tokens() {
    // "hello world" shares "hello" with old and "world" with new
    let score = token_overlap_score("hello world", "hello foo", "world bar");
    assert!(score > 0, "expected positive score, got {score}");
}

#[test]
fn token_overlap_score_zero_when_no_overlap() {
    let score = token_overlap_score("abc", "xyz", "uvw");
    assert_eq!(score, 0);
}

// ── differing_token_pair ───────────────────────────────────────────────

#[test]
fn differing_token_pair_single_diff_returns_pair() {
    let result = differing_token_pair("foo bar", "foo baz");
    assert_eq!(result, Some(("bar".to_string(), "baz".to_string())));
}

#[test]
fn differing_token_pair_returns_none_when_multiple_diffs() {
    let result = differing_token_pair("foo bar", "qux baz");
    assert_eq!(result, None);
}

#[test]
fn differing_token_pair_returns_none_when_token_counts_differ() {
    let result = differing_token_pair("foo", "foo bar");
    assert_eq!(result, None);
}

// ── attempt_macro_arg_patch ────────────────────────────────────────────

#[test]
fn attempt_macro_arg_patch_exact_col_replaces() {
    let ls = lines("    let x = old_val;");
    // old_text "old_val" starts at byte 12
    let result = attempt_macro_arg_patch(&ls, 0, 12, "old_val", "new_val");
    assert_eq!(result, Some("    let x = new_val;".to_string()));
}

#[test]
fn attempt_macro_arg_patch_returns_none_when_not_found() {
    let ls = lines("irrelevant line");
    let result = attempt_macro_arg_patch(&ls, 0, 0, "missing", "replacement");
    assert_eq!(result, None);
}

#[test]
fn attempt_macro_arg_patch_fallback_finds_differing_part() {
    // Source line has indentation, but src_col is 0.
    // Exact match at 0 fails, fallback scans for the differing part.
    let ls = lines("    let x = old_val;");
    let old_text = "let x = old_val;";
    let new_text = "let x = new_val;";
    // old/new differ at "old_val" vs "new_val". 
    // common prefix: "let x = " (8 chars)
    // common suffix: ";" (1 char)
    // old_frag: "old_val"
    let result = attempt_macro_arg_patch(&ls, 0, 0, old_text, new_text);
    assert_eq!(result, Some("    let x = new_val;".to_string()));
}

#[test]
fn attempt_macro_arg_patch_fallback_avoids_false_suffix_match() {
    // old: "literate", new: "illiterate"
    // prefix: "" (0), suffix: "literate" (8)
    // This is a tricky case because old is a suffix of new.
    let ls = lines("    process(literate);");
    let result = attempt_macro_arg_patch(&ls, 0, 0, "literate", "illiterate");
    assert_eq!(result, Some("    process(illiterate);".to_string()));
}

// ── attempt_macro_body_fix ─────────────────────────────────────────────

#[test]
fn attempt_macro_body_fix_no_vars_replaces_literal() {
    // Body has no %%(…) variables; old_expanded matches body_line
    let result = attempt_macro_body_fix("hello world", "hello world", "hello Rust", '%');
    assert_eq!(result, Some("hello Rust".to_string()));
}

#[test]
fn attempt_macro_body_fix_returns_none_when_same() {
    let result = attempt_macro_body_fix("foo", "foo", "foo", '%');
    assert_eq!(result, None);
}

#[test]
fn attempt_macro_body_fix_adjacent_vars_is_ambiguous() {
    // adjacent variables with no separator are rejected as ambiguous
    let result = attempt_macro_body_fix(
        "%(first)%(second)",
        "val1val2",
        "new1new2",
        '%'
    );
    assert_eq!(result, None);
}

#[test]
fn attempt_macro_body_fix_no_match_returns_none() {
    let result = attempt_macro_body_fix("completely different", "old", "new", '%');
    assert_eq!(result, None);
}

// ── ApplyBackError Display ─────────────────────────────────────────────

#[test]
fn apply_back_error_display_io_variant() {
    let e = ApplyBackError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"));
    let s = format!("{e}");
    assert!(s.contains("I/O error"), "got: {s}");
}

// ── patch_source_rank ──────────────────────────────────────────────────

#[test]
fn patch_source_rank_macro_arg_outranks_literal() {
    let arg = PatchSource::MacroArg {
        src_file: "f".into(), src_line: 1, src_col: 0,
        macro_name: "m".into(), param_name: "p".into(),
    };
    let lit = PatchSource::Literal { src_file: "f".into(), src_line: 1, len: 1 };
    assert!(patch_source_rank(&arg) > patch_source_rank(&lit));
}

#[test]
fn patch_source_rank_unpatchable_is_lowest() {
    let unp = PatchSource::Unpatchable { src_file: "f".into(), src_line: 1, kind_label: "x".into() };
    let nw = PatchSource::Noweb { src_file: "f".into(), src_line: 1, len: 1 };
    assert!(patch_source_rank(&unp) < patch_source_rank(&nw));
}
// ── patch_source_location ──────────────────────────────────────────────

#[test]
fn patch_source_location_returns_file_and_line() {
    let lit = PatchSource::Literal { src_file: "src/foo.adoc".into(), src_line: 42, len: 1 };
    let (file, line) = patch_source_location(&lit);
    assert_eq!(file, "src/foo.adoc");
    assert_eq!(line, 42);
}

// ── strip_indent ────────────────────────────────────────────────────────

#[test]
fn strip_indent_removes_prefix() {
    let result = strip_indent("    hello world", "    ");
    assert_eq!(result, "hello world");
}

#[test]
fn strip_indent_returns_original_when_no_match() {
    let result = strip_indent("hello", "    ");
    assert_eq!(result, "hello");
}

// ── verify_candidate ────────────────────────────────────────────────────

#[test]
fn verify_candidate_returns_true_for_matching_line() {
    // A simple inline macro chunk: "Hello world!" → expanded line 0 = "Hello world!"
    let src = "Hello world!\n";
    let config = EvalConfig::default();
    let path = std::path::Path::new("test.adoc");
    assert!(verify_candidate(src, path, &config, 0, "Hello world!"));
}

#[test]
fn verify_candidate_returns_false_for_mismatched_line() {
    let src = "Hello world!\n";
    let config = EvalConfig::default();
    let path = std::path::Path::new("test.adoc");
    assert!(!verify_candidate(src, path, &config, 0, "Goodbye world!"));
}

#[test]
fn verify_candidate_returns_false_when_line_out_of_range() {
    let src = "only one line\n";
    let config = EvalConfig::default();
    let path = std::path::Path::new("test.adoc");
    assert!(!verify_candidate(src, path, &config, 99, "only one line"));
}

// ── do_patch ─────────────────────────────────────────────────────────────

#[test]
fn do_patch_applies_exact_match() {
    let mut lines = lines("aaa\nbbb\nccc");
    let mut out = Vec::new();
    let mut skipped = 0;
    let mut applied = 0;
    let mut conflicts = 0;
    do_patch("f.adoc", 1, 1, "bbb", "BBB", &mut lines, false,
             &mut skipped, &mut applied, &mut conflicts, None, &mut out);
    assert_eq!(applied, 1);
    assert_eq!(lines[1], "BBB");
    let msg = String::from_utf8(out).unwrap();
    assert!(msg.contains("patched"));
}

#[test]
fn do_patch_detects_already_applied() {
    let mut lines = lines("aaa\nBBB\nccc");
    let mut out = Vec::new();
    let mut skipped = 0;
    let mut applied = 0;
    let mut conflicts = 0;
    do_patch("f.adoc", 1, 1, "bbb", "BBB", &mut lines, false,
             &mut skipped, &mut applied, &mut conflicts, None, &mut out);
    let msg = String::from_utf8(out).unwrap();
    assert!(msg.contains("already applied"));
}

#[test]
fn do_patch_records_conflict_when_no_match() {
    let mut lines = lines("aaa\nzzzz\nccc");
    let mut out = Vec::new();
    let mut skipped = 0;
    let mut applied = 0;
    let mut conflicts = 0;
    do_patch("f.adoc", 1, 1, "bbb", "BBB", &mut lines, false,
             &mut skipped, &mut applied, &mut conflicts, None, &mut out);
    assert_eq!(conflicts, 1);
    let msg = String::from_utf8(out).unwrap();
    assert!(msg.contains("CONFLICT"));
}

#[test]
fn do_patch_dry_run_does_not_modify_lines() {
    let mut lines = lines("aaa\nbbb\nccc");
    let mut out = Vec::new();
    let mut skipped = 0;
    let mut applied = 0;
    let mut conflicts = 0;
    do_patch("f.adoc", 1, 1, "bbb", "BBB", &mut lines, true,
             &mut skipped, &mut applied, &mut conflicts, None, &mut out);
    assert_eq!(lines[1], "bbb", "dry-run must not modify content");
    let msg = String::from_utf8(out).unwrap();
    assert!(msg.contains("dry-run"));
}

// ── rank_candidate ─────────────────────────────────────────────────────

#[test]
fn rank_candidate_closer_line_scores_higher() {
    let score_close = rank_candidate(5, 5, "old foo", "old foo", "new foo", 0);
    let score_far   = rank_candidate(5, 15, "old foo", "old foo", "new foo", 0);
    assert!(score_close > score_far);
}

// ── choose_best_candidate ──────────────────────────────────────────────

#[test]
fn choose_best_candidate_returns_highest_score() {
    let candidates = vec![
        CandidateResolution { line_idx: 0, new_line: "a".into(), score: 10 },
        CandidateResolution { line_idx: 1, new_line: "b".into(), score: 99 },
        CandidateResolution { line_idx: 2, new_line: "c".into(), score: 5  },
    ];
    let best = choose_best_candidate(candidates).unwrap();
    assert_eq!(best.line_idx, 1);
    assert_eq!(best.score, 99);
}

#[test]
fn choose_best_candidate_returns_none_on_tie() {
    let candidates = vec![
        CandidateResolution { line_idx: 0, new_line: "a".into(), score: 50 },
        CandidateResolution { line_idx: 1, new_line: "b".into(), score: 50 },
    ];
    assert!(choose_best_candidate(candidates).is_none());
}

#[test]
fn choose_best_candidate_returns_none_when_empty() {
    let result = choose_best_candidate(vec![]);
    assert!(result.is_none());
}

// ── run_apply_back with missing db ─────────────────────────────────────

#[test]
fn run_apply_back_reports_missing_database() {
    use std::path::PathBuf;
    let opts = ApplyBackOptions {
        db_path: PathBuf::from("/nonexistent/weaveback.db"),
        gen_dir: PathBuf::from("/nonexistent/gen"),
        dry_run: true,
        files: vec![],
        eval_config: None,
    };
    let mut out = Vec::new();
    let result = run_apply_back(opts, &mut out);
    assert!(result.is_ok());
    let msg = String::from_utf8(out).unwrap();
    assert!(msg.contains("Database not found"));
}

// ── attempt_macro_body_fix with vars ────────────────────────────────────

#[test]
fn attempt_macro_body_fix_with_single_var_updates_literal() {
    // body: "Hello %(name). Bye."
    // old expanded: "Hello Alice. Bye."
    // new expanded: "Hello Alice. Later."
    // Expects the literal suffix " Bye." → " Later." while preserving %(name).
    let result = attempt_macro_body_fix(
        "Hello %(name). Bye.",
        "Hello Alice. Bye.",
        "Hello Alice. Later.",
        '%',
    );
    assert!(result.is_some(), "expected Some result, got None");
    let new_body = result.unwrap();
    assert!(new_body.contains("Later"), "expected 'Later' in: {new_body}");
    assert!(new_body.contains("%(name)"), "expected var ref preserved in: {new_body}");
}

#[test]
fn attempt_macro_body_fix_returns_none_when_body_eq_expanded() {
    // body line IS exactly the expanded text → result is just the new expanded text
    let result = attempt_macro_body_fix("plain text", "plain text", "new text", '%');
    assert_eq!(result, Some("new text".to_string()));
}
// @
```

