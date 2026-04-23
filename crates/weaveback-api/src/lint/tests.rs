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
fn lint_accepts_chunk_inside_literal_block() {
    let text = "= Title\n\n....\n// <<alpha>>=\nbody\n// @\n....\n";
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
fn lint_syntaxes_match_pass_extension() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("weaveback.toml"),
        r#"
[[pass]]
dir = "docs/"
ext = "wvb"
open_delim = "<["
close_delim = "]>"
chunk_end = "@@"
comment_markers = "//"
"#,
    )
    .unwrap();

    let syntaxes = load_lint_syntaxes_from(temp.path());
    let adoc_syntaxes = lint_syntaxes_for_file(Path::new("./docs/page.adoc"), &syntaxes);
    let wvb_syntaxes = lint_syntaxes_for_file(Path::new("./docs/page.wvb"), &syntaxes);

    assert!(parse_chunk_definition_name("// <[alpha]>=", &adoc_syntaxes).is_none());
    assert_eq!(
        parse_chunk_definition_name("// <[alpha]>=", &wvb_syntaxes).as_deref(),
        Some("alpha")
    );
}

#[test]
fn collect_literate_files_skips_generated_dirs_and_includes_wvb() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("docs/html")).unwrap();
    fs::create_dir_all(temp.path().join("expanded-adoc")).unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src").join("ok.adoc"), "= Ok\n").unwrap();
    fs::write(temp.path().join("src").join("ok.wvb"), "¤h1(Ok)\n").unwrap();
    fs::write(temp.path().join("docs/html").join("skip.adoc"), "= Skip\n").unwrap();
    fs::write(temp.path().join("expanded-adoc").join("skip.wvb"), "= Skip\n").unwrap();

    let mut files = Vec::new();
    collect_literate_files(temp.path(), &mut files).unwrap();
    files.sort();
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|path| path.ends_with("ok.adoc")));
    assert!(files.iter().any(|path| path.ends_with("ok.wvb")));
}

#[test]
fn lint_raw_wvb_links_detects_adoc_links() {
    let text = "See link:target.adoc[target] and xref:other.adoc[other].\n";
    let violations = lint_raw_wvb_links(Path::new("sample.wvb"), text);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].rule, LintRule::RawWvbLink);
}

#[test]
fn lint_raw_wvb_links_ignores_neutral_links() {
    let text = "See \u{00a4}link(target.adoc, target) and \u{00a4}xref(other.adoc, other).\n";
    assert!(lint_raw_wvb_links(Path::new("sample.wvb"), text).is_empty());
}

#[test]
fn lint_raw_wvb_source_blocks_detects_direct_source_block() {
    let text = "Prose\n\n[source,rust]\n----\nfn main() {}\n----\n";
    let violations = lint_raw_wvb_source_blocks(Path::new("sample.wvb"), text);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].rule, LintRule::RawWvbSourceBlock);
}

#[test]
fn lint_raw_wvb_source_blocks_ignores_rust_string_literals() {
    let text = "let src = \"[source,rust]\\n----\\nfn main() {}\\n----\\n\";\n";
    assert!(lint_raw_wvb_source_blocks(Path::new("sample.wvb"), text).is_empty());
}

#[test]
fn lint_raw_wvb_tables_detects_unwrapped_table_fence() {
    let text = "[cols=\"1,1\"]\n|===\n| A | B\n|===\n";
    let violations = lint_raw_wvb_tables(Path::new("sample.wvb"), text);
    assert_eq!(violations.len(), 2);
    assert_eq!(violations[0].rule, LintRule::RawWvbTable);
}

#[test]
fn lint_raw_wvb_tables_accepts_explicit_table_macro() {
    let text = "\u{00a4}table(adoc, \u{00a4}[\n[cols=\"1,1\"]\n|===\n| A | B\n|===\n\u{00a4}])\n";
    assert!(lint_raw_wvb_tables(Path::new("sample.wvb"), text).is_empty());
}

#[test]
fn lint_raw_wvb_markup_ignores_prelude_implementation() {
    let text = "\u{00a4}redef(code_block, language, body, \u{00a4}{[source,\u{00a4}(language)]\n----\u{00a4}(body)----\n\u{00a4}})\n";
    assert!(lint_raw_wvb_source_blocks(Path::new("prelude/asciidoc.wvb"), text).is_empty());
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

