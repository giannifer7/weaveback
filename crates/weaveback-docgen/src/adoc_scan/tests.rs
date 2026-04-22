// weaveback-docgen/src/adoc_scan/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

#[test]
fn finds_source_d2_block() {
    let source = "= Title\n\n[source,d2]\n----\na -> b\n----\n";
    let blocks = collect_listing_blocks_by_language(source, "d2", "test");
    assert_eq!(blocks.len(), 1);
    assert!(blocks[0].content.contains("a -> b"));
    assert_eq!(&source[blocks[0].start..blocks[0].end], "[source,d2]\n----\na -> b\n----");
}

#[test]
fn finds_style_only_d2_block() {
    let source = "[d2]\n----\na -> b\n----\n";
    let blocks = collect_listing_blocks_by_language(source, "d2", "test");
    assert_eq!(blocks.len(), 1);
    assert!(blocks[0].content.contains("a -> b"));
}

#[test]
fn ignores_other_languages() {
    let source = "[source,rust]\n----\nfn main() {}\n----\n";
    let blocks = collect_listing_blocks_by_language(source, "d2", "test");
    assert!(blocks.is_empty());
}

#[test]
fn utf8_before_block_keeps_byte_ranges_valid() {
    let source = "éèø\n\n[source,d2]\n----\na -> b\n----\n";
    let blocks = collect_listing_blocks_by_language(source, "d2", "test");
    assert_eq!(blocks.len(), 1);
    assert_eq!(&source[blocks[0].start..blocks[0].end], "[source,d2]\n----\na -> b\n----");
}

#[test]
fn include_before_block_does_not_shift_offsets() {
    let source = "include::missing.adoc[]\n\n[source,d2]\n----\na -> b\n----\n";
    let blocks = collect_listing_blocks_by_language(source, "d2", "test");
    assert_eq!(blocks.len(), 1);
    assert_eq!(&source[blocks[0].start..blocks[0].end], "[source,d2]\n----\na -> b\n----");
}

#[test]
fn escaped_include_is_not_masked() {
    let source = "\\include::example.adoc[]\n";
    assert_eq!(mask_preprocessor_directives(source), source);
}

#[test]
fn masked_include_preserves_length() {
    let source = "include::example.adoc[]\n";
    let masked = mask_preprocessor_directives(source);
    assert_eq!(masked.len(), source.len());
    assert!(masked.starts_with("xnclude::"));
}

