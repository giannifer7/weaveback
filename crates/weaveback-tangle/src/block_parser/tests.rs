// weaveback-tangle/src/block_parser/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

#[test]
fn adoc_single_code_block() {
    let src = "= Title\n\n----\nfoo\n----\n\nProse.\n";
    let blocks = parse_source_blocks(src, "adoc");
    let types: Vec<_> = blocks.iter().map(|b| b.block_type.as_str()).collect();
    assert!(types.contains(&"code"), "expected code block, got {:?}", types);
    let code = blocks.iter().find(|b| b.block_type == "code").unwrap();
    assert_eq!(code.line_start, 3);
    assert_eq!(code.line_end, 5);
}

#[test]
fn adoc_two_code_blocks_have_different_hashes() {
    let src = "----\nfoo\n----\n\n----\nbar\n----\n";
    let blocks = parse_source_blocks(src, "adoc");
    let codes: Vec<_> = blocks.iter().filter(|b| b.block_type == "code").collect();
    assert_eq!(codes.len(), 2);
    assert_ne!(codes[0].content_hash, codes[1].content_hash);
}

#[test]
fn adoc_unchanged_block_same_hash() {
    let src = "----\nfoo\n----\n";
    let b1 = parse_source_blocks(src, "adoc");
    let b2 = parse_source_blocks(src, "adoc");
    assert_eq!(b1[0].content_hash, b2[0].content_hash);
}

#[test]
fn markdown_heading_and_code() {
    let src = "# Heading\n\n```rust\nfn main() {}\n```\n";
    let blocks = parse_source_blocks(src, "md");
    let types: Vec<_> = blocks.iter().map(|b| b.block_type.as_str()).collect();
    assert!(types.contains(&"section"), "expected section, got {:?}", types);
    assert!(types.contains(&"code"), "expected code, got {:?}", types);
}

#[test]
fn fallback_single_block() {
    let src = "line1\nline2\n";
    let blocks = parse_source_blocks(src, "rs");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].line_start, 1);
    assert_eq!(blocks[0].line_end, 2);
}

#[test]
fn adoc_section_and_para_are_split() {
    let src = "= Title\n\nIntro paragraph.\n\n== Next\n\nMore prose.\n";
    let blocks = parse_source_blocks(src, "adoc");
    let types: Vec<_> = blocks.iter().map(|b| b.block_type.as_str()).collect();
    assert!(types.contains(&"section"));
    assert!(types.iter().filter(|t| **t == "para").count() >= 2);
    assert!(blocks.iter().any(|b| b.block_type == "section" && b.line_start == 5 && b.line_end == 5));
    assert!(blocks.iter().any(|b| b.block_type == "para" && b.line_start == 3 && b.line_end == 3));
    assert!(blocks.iter().any(|b| b.block_type == "para" && b.line_start == 7 && b.line_end == 7));
}

#[test]
fn adoc_unclosed_fence_runs_to_end_of_file() {
    let src = "= Title\n\n----\nfn main() {}\n";
    let blocks = parse_source_blocks(src, "adoc");
    let code = blocks.iter().find(|b| b.block_type == "code").unwrap();
    assert_eq!(code.line_start, 3);
    assert_eq!(code.line_end, 4);
}

#[test]
fn detects_unclosed_adoc_fence() {
    assert!(has_unclosed_adoc_fence("----\ncode\n"));
    assert!(!has_unclosed_adoc_fence("----\ncode\n----\n"));
}

#[test]
fn markdown_paragraphs_are_emitted() {
    let src = "# Heading\n\nAlpha paragraph.\n\nBeta paragraph.\n";
    let blocks = parse_source_blocks(src, "md");
    let types: Vec<_> = blocks.iter().map(|b| b.block_type.as_str()).collect();
    assert!(types.contains(&"section"));
    assert_eq!(types.iter().filter(|t| **t == "para").count(), 2);
}

#[test]
fn empty_markdown_falls_back_to_text_block() {
    let blocks = parse_source_blocks("", "md");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].block_type, "text");
    assert_eq!(blocks[0].line_start, 1);
    assert_eq!(blocks[0].line_end, 1);
}

// ---- new coverage tests ----

#[test]
fn asciidoc_extension_alias_works() {
    // ".asciidoc" should behave identically to ".adoc"
    let src = "----\nhello\n----\n";
    let blocks = parse_source_blocks(src, "asciidoc");
    assert!(blocks.iter().any(|b| b.block_type == "code"));
}

#[test]
fn markdown_extension_alias_works() {
    // ".markdown" should behave identically to ".md"
    let src = "# Heading\n\nProse.\n";
    let blocks = parse_source_blocks(src, "markdown");
    let types: Vec<_> = blocks.iter().map(|b| b.block_type.as_str()).collect();
    assert!(types.contains(&"section"));
}

#[test]
fn fallback_empty_source_single_block() {
    let blocks = parse_source_blocks("", "rs");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].line_start, 1);
    assert_eq!(blocks[0].line_end, 1);
}

#[test]
fn is_adoc_fence_dot_fence() {
    assert!(is_adoc_fence("...."));
    assert!(is_adoc_fence("........"));
    assert!(!is_adoc_fence("...x"));
}

#[test]
fn is_adoc_fence_plus_fence() {
    assert!(is_adoc_fence("++++"));
    assert!(!is_adoc_fence("+++-"));
}

#[test]
fn is_adoc_fence_dash_fence() {
    assert!(is_adoc_fence("----"));
    assert!(is_adoc_fence("--------"));
    assert!(!is_adoc_fence("---x"));
}

#[test]
fn is_adoc_section_header_various() {
    assert!(is_adoc_section_header("= Title"));
    assert!(is_adoc_section_header("== Section"));
    assert!(is_adoc_section_header("=== Sub"));
    assert!(is_adoc_section_header("="));
    assert!(!is_adoc_section_header("not a header"));
    assert!(!is_adoc_section_header("=x no space"));
}

#[test]
fn adoc_dot_fence_parsed_as_code() {
    let src = "....\nsome listing\n....\n";
    let blocks = parse_source_blocks(src, "adoc");
    assert!(blocks.iter().any(|b| b.block_type == "code"));
}

#[test]
fn adoc_plus_fence_parsed_as_code() {
    let src = "++++\npassthrough\n++++\n";
    let blocks = parse_source_blocks(src, "adoc");
    assert!(blocks.iter().any(|b| b.block_type == "code"));
}

#[test]
fn adoc_empty_source_produces_block() {
    let blocks = parse_source_blocks("", "adoc");
    assert!(!blocks.is_empty());
    assert_eq!(blocks[0].line_start, 1);
}

#[test]
fn adoc_include_before_code_does_not_shift_line_range() {
    let src = "include::missing.adoc[]\n\n[source,rust]\n----\nfn main() {}\n----\n";
    let blocks = parse_source_blocks(src, "adoc");
    let code = blocks.iter().find(|b| b.block_type == "code").unwrap();
    assert_eq!(code.line_start, 3);
    assert_eq!(code.line_end, 6);
}

#[test]
fn adoc_utf8_before_code_keeps_line_range() {
    let src = "éèø\n\n[source,rust]\n----\nfn main() {}\n----\n";
    let blocks = parse_source_blocks(src, "adoc");
    let code = blocks.iter().find(|b| b.block_type == "code").unwrap();
    assert_eq!(code.line_start, 3);
    assert_eq!(code.line_end, 6);
}

#[test]
fn block_index_is_sequential() {
    let src = "# H\n\nPara one.\n\nPara two.\n";
    let blocks = parse_source_blocks(src, "md");
    for (i, b) in blocks.iter().enumerate() {
        assert_eq!(b.block_index, i as u32);
    }
}

