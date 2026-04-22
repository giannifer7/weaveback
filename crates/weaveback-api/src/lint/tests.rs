// weaveback-api/src/lint/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use tempfile::TempDir;

#[test]
fn lint_detects_chunk_outside_fence() {
    let text = "= Title\n\n// <<alpha>>=\nbody\n// @\n";
    let syntax = NowebSyntax::new(
        "<<",
        ">>",
        "@",
        &["#".to_string(), "//".to_string()],
    );
    let syntaxes = vec![&syntax];
    let violations = lint_chunk_body_outside_fence(Path::new("sample.adoc"), text, &syntaxes);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].line, 3);
    assert_eq!(violations[0].rule, LintRule::ChunkBodyOutsideFence);
}

#[test]
fn lint_accepts_chunk_inside_fence() {
    let text = "= Title\n\n----\n// <<alpha>>=\nbody\n// @\n----\n";
    let syntax = NowebSyntax::new(
        "<<",
        ">>",
        "@",
        &["#".to_string(), "//".to_string()],
    );
    let syntaxes = vec![&syntax];
    assert!(lint_chunk_body_outside_fence(Path::new("sample.adoc"), text, &syntaxes).is_empty());
}

#[test]
fn lint_detects_unterminated_chunk_at_end_of_file() {
    let text = "= Title\n\n----\n// <<alpha>>=\nbody\n----\n";
    let syntax = NowebSyntax::new(
        "<<",
        ">>",
        "@",
        &["#".to_string(), "//".to_string()],
    );
    let syntaxes = vec![&syntax];
    let violations =
        lint_unterminated_chunk_definition(Path::new("sample.adoc"), text, &syntaxes);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].line, 4);
    assert_eq!(violations[0].rule, LintRule::UnterminatedChunkDefinition);
}

#[test]
fn lint_detects_new_chunk_before_previous_close() {
    let text = "= Title\n\n----\n// <<alpha>>=\nbody\n// <<beta>>=\nmore\n// @\n----\n";
    let syntax = NowebSyntax::new(
        "<<",
        ">>",
        "@",
        &["#".to_string(), "//".to_string()],
    );
    let syntaxes = vec![&syntax];
    let violations =
        lint_unterminated_chunk_definition(Path::new("sample.adoc"), text, &syntaxes);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].message.contains("alpha"));
}

#[test]
fn lint_detects_hash_prefixed_angle_bracket_chunks() {
    let text = "= Title\n\n# <[alpha]>=\nbody\n# @@\n";
    let syntax = NowebSyntax::new(
        "<[",
        "]>",
        "@@",
        &["#".to_string(), "//".to_string()],
    );
    let syntaxes = vec![&syntax];
    let violations = lint_chunk_body_outside_fence(Path::new("sample.adoc"), text, &syntaxes);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].message.contains("alpha"));
}

#[test]
fn lint_detects_uncommented_triple_angle_chunks() {
    let text = "= Title\n\n<<<alpha>>>=\nbody\n@@\n";
    let syntax = NowebSyntax::new("<<<", ">>>", "@@", &["//".to_string()]);
    let syntaxes = vec![&syntax];
    let violations = lint_chunk_body_outside_fence(Path::new("sample.adoc"), text, &syntaxes);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].message.contains("alpha"));
}

#[test]
fn load_lint_syntaxes_reads_pass_syntax_from_config() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("weaveback.toml"),
        r#"
[[pass]]
dir = "demo/"
open_delim = "<["
close_delim = "]>"
chunk_end = "@@"
comment_markers = "//"
"#,
    )
    .unwrap();
    let syntaxes = load_lint_syntaxes_from(temp.path());
    let syntaxes = syntaxes.iter().map(|entry| &entry.syntax).collect::<Vec<_>>();

    assert!(
        parse_chunk_definition_name("// <[alpha]>=", &syntaxes)
            .as_deref()
            == Some("alpha")
    );
}

#[test]
fn lint_uses_matching_pass_syntax_only() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("weaveback.toml"),
        r#"
[[pass]]
dir = "examples/"
open_delim = "<["
close_delim = "]>"
chunk_end = "@@"
comment_markers = "//"
"#,
    )
    .unwrap();
    fs::create_dir_all(temp.path().join("examples")).unwrap();
    let file = temp.path().join("README.adoc");
    fs::write(&file, "----\n// <[alpha]>=\nbody\n// @\n----\n").unwrap();

    let syntaxes = load_lint_syntaxes_from(temp.path());
    let file_syntaxes = lint_syntaxes_for_file(Path::new("./README.adoc"), &syntaxes);

    assert!(parse_chunk_definition_name("// <[alpha]>=", &file_syntaxes).is_none());
}

#[test]
fn collect_adoc_files_skips_generated_dirs() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("docs/html")).unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src").join("ok.adoc"), "= Ok\n").unwrap();
    fs::write(temp.path().join("docs/html").join("skip.adoc"), "= Skip\n").unwrap();

    let mut files = Vec::new();
    collect_adoc_files(temp.path(), &mut files).unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("ok.adoc"));
}

#[test]
fn run_lint_is_warning_by_default_and_error_in_strict_mode() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("bad.adoc");
    fs::write(&path, "// <<alpha>>=\nbody\n// @\n").unwrap();

    assert!(run_lint(vec![path.clone()], false, None, false).is_ok());
    let err = run_lint(vec![path], true, None, false).unwrap_err();
    assert!(err.contains("1 violation"));
}

#[test]
fn run_lint_can_emit_json() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("bad.adoc");
    fs::write(&path, "// <<alpha>>=\nbody\n// @\n").unwrap();

    assert!(run_lint(vec![path], false, None, true).is_ok());
}

