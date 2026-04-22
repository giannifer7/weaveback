// weaveback-docgen/src/plantuml/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use tempfile::TempDir;

#[test]
fn normalize_svg_background_replaces_uppercase() {
    let input = b"<svg><style>background:#FFFFFF;</style></svg>".to_vec();
    let out = normalize_svg_background(input);
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("background:transparent;"));
    assert!(!s.contains("#FFFFFF"));
}

#[test]
fn normalize_svg_background_replaces_lowercase() {
    let input = b"<svg><style>background:#ffffff;</style></svg>".to_vec();
    let out = normalize_svg_background(input);
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("background:transparent;"));
    assert!(!s.contains("#ffffff"));
}

#[test]
fn normalize_svg_background_non_utf8_passthrough() {
    let input: Vec<u8> = vec![0xFF, 0xFE, 0x00];
    let out = normalize_svg_background(input.clone());
    assert_eq!(out, input);
}

#[test]
fn normalize_svg_background_no_match_unchanged() {
    let input = b"<svg>no background here</svg>".to_vec();
    let out = normalize_svg_background(input.clone());
    assert_eq!(out, input);
}

#[test]
fn batch_render_plantuml_empty_returns_ok() {
    let fake_jar = std::path::Path::new("/nonexistent/plantuml.jar");
    let result = batch_render_plantuml(&[], fake_jar);
    assert!(result.is_ok());
}

#[test]
fn preprocess_plantuml_no_blocks_returns_none() {
    let source = "= My Document\n\nJust plain text, no diagrams.";
    let tmp = TempDir::new().unwrap();
    let fake_jar = tmp.path().join("plantuml.jar");
    let result = preprocess_plantuml(
        source,
        &fake_jar,
        tmp.path(),
        tmp.path(),
        "test.adoc",
    );
    assert!(matches!(result, Ok(None)));
}

#[test]
fn collect_uncached_diagrams_empty_source_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let diagrams = collect_uncached_plantuml_diagrams("plain text", tmp.path(), "test");
    assert!(diagrams.is_empty());
}

#[test]
fn plantuml_error_exit_failure_display() {
    let err = PlantUmlError::ExitFailure { code: 1, index: 0 };
    let msg = err.to_string();
    assert!(msg.contains("status 1"));
    assert!(msg.contains("#0"));
}

#[test]
fn plantuml_error_batch_failed_display() {
    let err = PlantUmlError::BatchFailed { code: 2 };
    let msg = err.to_string();
    assert!(msg.contains("2"));
}

#[test]
fn normalize_svg_file_in_place_modifies_uppercase_background() {
    let tmp = TempDir::new().unwrap();
    let svg_path = tmp.path().join("test.svg");
    std::fs::write(&svg_path, b"<svg>background:#FFFFFF;</svg>").unwrap();
    normalize_svg_file_in_place(&svg_path).unwrap();
    let content = std::fs::read_to_string(&svg_path).unwrap();
    assert!(content.contains("background:transparent;"));
}

#[test]
fn collect_plantuml_blocks_extracts_plantuml_blocks() {
    let src = "= Title\n\n[source,plantuml]\n----\nA -> B\n----\n";
    let blocks = collect_plantuml_blocks(src, "test");
    assert_eq!(blocks.len(), 1);
    assert!(blocks[0].2.contains("A -> B"));
}

#[test]
fn collect_plantuml_blocks_ignores_non_plantuml_blocks() {
    let src = "[source,rust]\n----\nfn main() {}\n----\n";
    let blocks = collect_plantuml_blocks(src, "test");
    assert!(blocks.is_empty());
}

#[test]
fn style_only_block_identifies_plantuml() {
    let src = "[plantuml]\n----\nx\n----\n";
    let blocks = collect_plantuml_blocks(src, "test");
    assert_eq!(blocks.len(), 1);
}

#[test]
fn collect_plantuml_blocks_offsets_survive_include_directive() {
    let src = "include::missing.adoc[]\n\n[source,plantuml]\n----\nA -> B\n----\n";
    let blocks = collect_plantuml_blocks(src, "test");
    assert_eq!(blocks.len(), 1);
    assert_eq!(&src[blocks[0].0..blocks[0].1], "[source,plantuml]\n----\nA -> B\n----");
}

