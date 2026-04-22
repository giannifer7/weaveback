# AsciiDoc scanner

`adoc_scan.rs` is the ACDC-backed structural scanner used by docgen features
that need source-accurate byte ranges.

The important constraint is offset stability.  ACDC normally runs the AsciiDoc
preprocessor before parsing; that is correct for rendering, but unsafe when the
resulting byte ranges are later applied to the original source string.  This
module therefore parses a same-length masked copy of the input.  Preprocessor
directive lines are replaced with spaces while preserving newline bytes.

## Public result


```rust
// <[adoc-scan-types]>=
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdocListingBlock {
    pub start: usize,
    pub end: usize,
    pub content: String,
}
// @
```


## Entry point


```rust
// <[adoc-scan-entry]>=
pub(crate) fn collect_listing_blocks_by_language(
    source: &str,
    language: &str,
    label: &str,
) -> Vec<AdocListingBlock> {
    let masked = mask_preprocessor_directives(source);
    let doc = match acdc_parser::Parser::new(&masked).parse() {
        Ok(doc) => doc,
        Err(err) => {
            eprintln!(
                "adoc scan: {label}: ACDC failed while scanning for {language} blocks: {err}"
            );
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    collect_from_blocks(&doc.blocks, source, language, &mut out);
    out
}
// @
```


## AST walk


```rust
// <[adoc-scan-walk]>=
fn collect_from_blocks(
    blocks: &[acdc_parser::Block],
    source: &str,
    language: &str,
    out: &mut Vec<AdocListingBlock>,
) {
    use acdc_parser::{Block, DelimitedBlockType};

    for block in blocks {
        match block {
            Block::Section(section) => {
                collect_from_blocks(&section.content, source, language, out);
            }
            Block::DelimitedBlock(delimited) => {
                let is_verbatim = matches!(
                    delimited.inner,
                    DelimitedBlockType::DelimitedListing(_)
                        | DelimitedBlockType::DelimitedLiteral(_)
                        | DelimitedBlockType::DelimitedPass(_)
                );

                if is_verbatim && block_language(delimited).as_deref() == Some(language) {
                    if let Some(content) = delimited_content(source, delimited) {
                        let end = delimited
                            .location
                            .absolute_end
                            .saturating_add(1)
                            .min(source.len());
                        out.push(AdocListingBlock {
                            start: delimited.location.absolute_start,
                            end,
                            content,
                        });
                    }
                    continue;
                }

                match &delimited.inner {
                    DelimitedBlockType::DelimitedExample(inner)
                    | DelimitedBlockType::DelimitedOpen(inner)
                    | DelimitedBlockType::DelimitedSidebar(inner)
                    | DelimitedBlockType::DelimitedQuote(inner) => {
                        collect_from_blocks(inner, source, language, out);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
// @
```


## Language detection

ACDC stores `[d2]` as the block style.  It stores `[source,d2]` as style
`source` plus a positional attribute that has been moved into element
attributes.  The language detector accepts both forms.


```rust
// <[adoc-scan-language]>=
fn block_language(block: &acdc_parser::DelimitedBlock) -> Option<String> {
    let style = block.metadata.style.as_deref()?;
    if style != "source" {
        return Some(style.to_string());
    }

    for (name, value) in block.metadata.attributes.iter() {
        if matches!(value, acdc_parser::AttributeValue::None) {
            return Some(name.clone());
        }
    }

    block
        .metadata
        .attributes
        .get_string("language")
        .or_else(|| block.metadata.attributes.get_string("lang"))
}
// @
```


## Original content slicing

The AST tells us where the delimited block starts and where its delimiter
lines are.  The diagram body is then sliced from the original source, not from
ACDC's inline representation.


```rust
// <[adoc-scan-content]>=
fn delimited_content(source: &str, block: &acdc_parser::DelimitedBlock) -> Option<String> {
    let open = block.open_delimiter_location.as_ref()?;
    let close = block.close_delimiter_location.as_ref()?;
    let content_start = source[open.absolute_start.min(source.len())..]
        .find('\n')
        .map(|offset| open.absolute_start + offset + 1)
        .unwrap_or(open.absolute_end.min(source.len()));
    let content_end = close.absolute_start.min(source.len());

    if content_start > content_end || !source.is_char_boundary(content_start) {
        return None;
    }
    if !source.is_char_boundary(content_end) {
        return None;
    }

    Some(source[content_start..content_end].to_string())
}
// @
```


## Preprocessor masking

The mask preserves every byte position.  The first non-whitespace byte of a
directive line is changed to `x`, which keeps the line as ordinary paragraph
text while preventing ACDC's preprocessor from recognizing it.  Replacing the
line with spaces is not safe because parser normalization may trim it and shift
subsequent offsets.


```rust
// <[adoc-scan-mask]>=
fn mask_preprocessor_directives(source: &str) -> String {
    let mut masked = String::with_capacity(source.len());

    for segment in source.split_inclusive('\n') {
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        if is_preprocessor_directive(line) {
            masked.push_str(&neutralize_preprocessor_directive(line));
        } else {
            masked.push_str(line);
        }
        if segment.ends_with('\n') {
            masked.push('\n');
        }
    }

    masked
}

fn is_preprocessor_directive(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with('\\') || trimmed.starts_with("//") {
        return false;
    }

    let Some((name, rest)) = trimmed.split_once("::") else {
        return false;
    };
    matches!(
        name,
        "include"
            | "ifdef"
            | "ifndef"
            | "ifeval"
            | "endif"
            | "else"
            | "elsifdef"
            | "elsifndef"
    ) && rest.ends_with(']')
}

fn neutralize_preprocessor_directive(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut replaced = false;

    for ch in line.chars() {
        if !replaced && !ch.is_whitespace() {
            out.push('x');
            replaced = true;
        } else {
            out.push(ch);
        }
    }

    out
}
// @
```


## Tests

The test body is generated as `adoc_scan/tests.rs` and linked from
`adoc_scan.rs` with `#[cfg(test)] mod tests;`.


```rust
// <[@file weaveback-docgen/src/adoc_scan/tests.rs]>=
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

// @
```


## Assembly


```rust
// <[@file weaveback-docgen/src/adoc_scan.rs]>=
// weaveback-docgen/src/adoc_scan.rs
// I'd Really Rather You Didn't edit this generated file.

// <[adoc-scan-types]>
// <[adoc-scan-entry]>
// <[adoc-scan-walk]>
// <[adoc-scan-language]>
// <[adoc-scan-content]>
// <[adoc-scan-mask]>
#[cfg(test)]
mod tests;

// @
```

