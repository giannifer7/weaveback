/// Sub-file block parsing for incremental build support.
///
/// Splits a source file into logical blocks (code blocks, section headers,
/// prose paragraphs) and computes a BLAKE3 hash for each block.  The hashes
/// are stored in the database so that unchanged blocks can be skipped on the
/// next run.
///
/// A parsed logical block with its line range and content hash.
#[derive(Debug, Clone)]
pub struct SourceBlockEntry {
    pub block_index: u32,
    pub block_type: String,
    pub line_start: u32, // 1-based, inclusive
    pub line_end: u32,   // 1-based, inclusive
    pub content_hash: [u8; 32],
}

/// Parse `source` into logical blocks based on its file `extension`.
///
/// Recognised extensions: `adoc`, `asciidoc` (AsciiDoc line scanner);
/// `md`, `markdown` (pulldown-cmark); everything else gets a single block.
pub fn parse_source_blocks(source: &str, extension: &str) -> Vec<SourceBlockEntry> {
    let raw = match extension {
        "adoc" | "asciidoc" => parse_adoc_raw(source),
        "md" | "markdown" => parse_markdown_raw(source),
        _ => {
            let n = source.lines().count().max(1) as u32;
            vec![(1, n, "text", source.to_string())]
        }
    };

    raw.into_iter()
        .enumerate()
        .map(|(i, (start, end, btype, content))| {
            let mut h = blake3::Hasher::new();
            h.update(content.as_bytes());
            SourceBlockEntry {
                block_index: i as u32,
                block_type: btype.to_string(),
                line_start: start,
                line_end: end,
                content_hash: *h.finalize().as_bytes(),
            }
        })
        .collect()
}

// ── AsciiDoc ──────────────────────────────────────────────────────────────────

/// Scan an AsciiDoc document line by line, splitting it into:
/// * `"section"` — a single `== …` header line
/// * `"code"`    — the content of a `----` delimited block (inclusive of delimiters)
/// * `"para"`    — a run of consecutive non-empty lines that are neither a
///   section header nor a delimiter
///
/// Each tuple is `(line_start, line_end, block_type, content)` (1-based lines).
fn parse_adoc_raw(source: &str) -> Vec<(u32, u32, &'static str, String)> {
    let mut blocks: Vec<(u32, u32, &'static str, String)> = Vec::new();

    let mut in_delim = false;
    let mut delim_start = 0u32;
    let mut delim_buf = String::new();

    let mut para_start = 0u32;
    let mut para_buf = String::new();

    let flush_para = |para_start: u32,
                      para_buf: &mut String,
                      current_line: u32,
                      blocks: &mut Vec<_>| {
        if !para_buf.is_empty() {
            let content = std::mem::take(para_buf);
            let end = current_line - 1;
            blocks.push((para_start, end.max(para_start), "para", content));
        }
    };

    for (idx, line) in source.lines().enumerate() {
        let lineno = idx as u32 + 1;

        if in_delim {
            delim_buf.push_str(line);
            delim_buf.push('\n');
            if is_adoc_fence(line) {
                // Closing delimiter — emit code block.
                let content = std::mem::take(&mut delim_buf);
                blocks.push((delim_start, lineno, "code", content));
                in_delim = false;
            }
            continue;
        }

        // Not in a delimited block.
        if is_adoc_fence(line) {
            // Flush any pending paragraph before starting a code block.
            flush_para(para_start, &mut para_buf, lineno, &mut blocks);
            in_delim = true;
            delim_start = lineno;
            delim_buf.push_str(line);
            delim_buf.push('\n');
            continue;
        }

        if is_adoc_section_header(line) {
            // Flush paragraph, emit section block.
            flush_para(para_start, &mut para_buf, lineno, &mut blocks);
            blocks.push((lineno, lineno, "section", line.to_string()));
            continue;
        }

        if line.trim().is_empty() {
            // Blank line: flush paragraph.
            flush_para(para_start, &mut para_buf, lineno, &mut blocks);
            continue;
        }

        // Accumulate into a prose paragraph.
        if para_buf.is_empty() {
            para_start = lineno;
        }
        para_buf.push_str(line);
        para_buf.push('\n');
    }

    // Flush any trailing paragraph or unclosed delimiter.
    let total_lines = source.lines().count() as u32;
    if in_delim && !delim_buf.is_empty() {
        blocks.push((delim_start, total_lines, "code", delim_buf));
    } else if !para_buf.is_empty() {
        blocks.push((para_start, total_lines, "para", para_buf));
    }

    blocks
}

fn is_adoc_fence(line: &str) -> bool {
    let t = line.trim_end();
    (t.starts_with("----") && t.chars().all(|c| c == '-'))
        || (t.starts_with("....") && t.chars().all(|c| c == '.'))
        || (t.starts_with("++++") && t.chars().all(|c| c == '+'))
}

fn is_adoc_section_header(line: &str) -> bool {
    let mut chars = line.chars();
    if chars.next() != Some('=') {
        return false;
    }
    // At least one more `=` then a space, OR a bare `=` title
    let rest: String = chars.collect();
    let trimmed = rest.trim_start_matches('=');
    trimmed.starts_with(' ') || trimmed.is_empty()
}

// ── Markdown ──────────────────────────────────────────────────────────────────

/// Parse Markdown using pulldown-cmark's offset iterator.
///
/// Produces blocks of type:
/// * `"section"` — a heading
/// * `"code"`    — a fenced code block
/// * `"para"`    — a paragraph or other leaf element
fn parse_markdown_raw(source: &str) -> Vec<(u32, u32, &'static str, String)> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};

    // Build a byte→line lookup table.
    let line_of_byte = build_line_table(source);

    let parser = Parser::new(source).into_offset_iter();
    let mut blocks: Vec<(u32, u32, &'static str, String)> = Vec::new();

    let mut depth = 0usize; // nesting depth so we skip inner events
    let mut cur_type: Option<&'static str> = None;
    let mut cur_start = 0usize;

    for (event, range) in parser {
        match event {
            Event::Start(tag) => {
                depth += 1;
                if depth == 1 {
                    let btype = match &tag {
                        Tag::Heading { .. } => "section",
                        Tag::CodeBlock(_) => "code",
                        _ => "para",
                    };
                    cur_type = Some(btype);
                    cur_start = range.start;
                }
            }
            Event::End(end_tag) => {
                if depth == 1
                    && let Some(btype) = cur_type.take() {
                        // Only emit code / section at depth == 1
                        let emit = matches!(end_tag, TagEnd::Heading(_) | TagEnd::CodeBlock)
                            || btype == "para";
                        if emit {
                            let byte_end = range.end;
                            let start_line = byte_to_line(&line_of_byte, cur_start);
                            let end_line = byte_to_line(&line_of_byte, byte_end.saturating_sub(1));
                            let content = source[cur_start..byte_end.min(source.len())].to_string();
                            blocks.push((start_line, end_line, btype, content));
                        }
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    if blocks.is_empty() {
        let n = source.lines().count().max(1) as u32;
        blocks.push((1, n, "text", source.to_string()));
    }
    blocks
}

/// Map of byte offset → 1-based line number.
fn build_line_table(source: &str) -> Vec<usize> {
    let mut table = Vec::with_capacity(source.len() + 1);
    let mut line = 1usize;
    for byte in source.bytes() {
        table.push(line);
        if byte == b'\n' {
            line += 1;
        }
    }
    table.push(line); // sentinel for end-of-file
    table
}

fn byte_to_line(table: &[usize], byte: usize) -> u32 {
    table.get(byte).copied().unwrap_or(1) as u32
}

#[cfg(test)]
mod tests {
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
}
