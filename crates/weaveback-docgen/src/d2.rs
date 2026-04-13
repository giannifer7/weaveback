use std::path::Path;
use std::io::Write;

#[derive(Debug, thiserror::Error)]
pub enum D2Error {
    #[error("failed to spawn d2: {0}")]
    Spawn(std::io::Error),

    #[error("failed to write to d2 stdin: {0}")]
    Stdin(std::io::Error),

    #[error("d2 exited with status {code} for diagram #{index}")]
    ExitFailure { code: i32, index: usize, stderr: String },

    #[error("failed to write SVG cache file '{path}': {source}")]
    CacheWrite { path: String, source: std::io::Error },
}

impl D2Error {
    fn is_missing_binary(&self) -> bool {
        matches!(self, D2Error::Spawn(e) if e.kind() == std::io::ErrorKind::NotFound)
    }
}
fn collect_d2_blocks(source: &str, label: &str) -> Vec<(usize, usize, String)> {
    use asciidoc_parser::{Parser, blocks::IsBlock};

    let parse_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Parser::default().parse(source)
    }));
    let doc = match parse_result {
        Ok(d) => d,
        Err(_) => {
            eprintln!("d2: {label}: asciidoc-parser panicked while scanning for d2 blocks — skipping d2 pre-processing for this file");
            return Vec::new();
        }
    };
    let mut results = Vec::new();
    collect_from_blocks(doc.nested_blocks(), &mut results);
    results
}

fn collect_from_blocks<'src>(
    blocks: std::slice::Iter<'src, asciidoc_parser::blocks::Block<'src>>,
    out: &mut Vec<(usize, usize, String)>,
) {
    use asciidoc_parser::{HasSpan, blocks::{Block, IsBlock as _}};

    for block in blocks {
        if let Block::RawDelimited(rdb) = block {
            let is_d2 = raw_block_language(rdb.attrlist())
                .map(|s| s == "d2")
                .unwrap_or(false);
            if is_d2 {
                let span = rdb.span();
                let start = span.byte_offset();
                let end = start + span.data().len();
                let diagram_src = rdb.content().original().data().to_owned();
                out.push((start, end, diagram_src));
                continue;
            }
        }
        collect_from_blocks(block.nested_blocks(), out);
    }
}

fn raw_block_language<'src>(
    attrlist: Option<&'src asciidoc_parser::attributes::Attrlist<'src>>,
) -> Option<&'src str> {
    let attrlist = attrlist?;
    let mut attrs = attrlist.attributes();
    let style = attrs.next()?.value();
    if style == "source" {
        attrs.next().map(|attr| attr.value())
    } else {
        Some(style)
    }
}
pub fn render_d2_diagram(
    diagram_source: &str,
    index: usize,
    theme: u32,
    layout: &str,
) -> Result<Vec<u8>, D2Error> {
    let mut child = std::process::Command::new("d2")
        .args([
            "--layout", layout,
            "--theme", &theme.to_string(),
            "-",
            "-",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(D2Error::Spawn)?;

    child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(diagram_source.as_bytes())
        .map_err(D2Error::Stdin)?;

    let output = child.wait_with_output().map_err(D2Error::Spawn)?;

    if !output.status.success() {
        return Err(D2Error::ExitFailure {
            code: output.status.code().unwrap_or(-1),
            index,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    Ok(output.stdout)
}
pub fn preprocess_d2(
    source: &str,
    images_out_dir: &Path,
    svg_cache_dir: &Path,
    label: &str,
    theme: u32,
    layout: &str,
) -> Result<Option<String>, D2Error> {
    let blocks = collect_d2_blocks(source, label);
    if blocks.is_empty() {
        return Ok(None);
    }

    std::fs::create_dir_all(images_out_dir).ok();

    let mut replacements: Vec<(usize, usize, String)> = Vec::with_capacity(blocks.len());

    for (index, (start, end, diagram_src)) in blocks.into_iter().enumerate() {
        let hash = blake3::hash(diagram_src.as_bytes());
        let svg_name = format!("d2-{}.svg", hash.to_hex());
        let svg_cache_path = svg_cache_dir.join(&svg_name);
        let svg_out_path = images_out_dir.join(&svg_name);

        if !svg_cache_path.exists() {
            let svg_bytes = match render_d2_diagram(&diagram_src, index, theme, layout) {
                Ok(svg) => svg,
                Err(e) if e.is_missing_binary() => {
                    eprintln!("d2: {label}: {e}; skipping D2 pre-processing for this file");
                    return Ok(None);
                }
                Err(e) => return Err(e),
            };
            std::fs::write(&svg_cache_path, &svg_bytes).map_err(|e| D2Error::CacheWrite {
                path: svg_cache_path.to_string_lossy().into_owned(),
                source: e,
            })?;
        }

        if !svg_out_path.exists() {
            std::fs::copy(&svg_cache_path, &svg_out_path).map_err(|e| D2Error::CacheWrite {
                path: svg_out_path.to_string_lossy().into_owned(),
                source: e,
            })?;
        }

        let replacement = format!("image::{svg_name}[D2 diagram]\n");
        replacements.push((start, end, replacement));
    }

    let mut result = source.to_owned();
    for (start, end, replacement) in replacements.into_iter().rev() {
        result.replace_range(start..end, &replacement);
    }

    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_d2_returns_none_when_no_d2_blocks() {
        let src = "= Title\n\nJust prose, no d2 blocks.\n\n----\nsome code\n----\n";
        let tmp = tempfile::TempDir::new().unwrap();
        let result = preprocess_d2(src, tmp.path(), tmp.path(), "test", 0, "dagre").unwrap();
        assert!(result.is_none(), "expected None when no d2 blocks present");
    }

    #[test]
    fn collect_d2_blocks_finds_no_blocks_in_plain_adoc() {
        let src = "= Title\n\n----\nsome code\n----\n";
        let blocks = collect_d2_blocks(src, "test");
        assert!(blocks.is_empty());
    }

    #[test]
    fn collect_d2_blocks_finds_d2_source_block() {
        // [source,d2] fence is recognized as a d2 block
        let src = "= Title\n\n[source,d2]\n----\na -> b\n----\n";
        let blocks = collect_d2_blocks(src, "test");
        assert_eq!(blocks.len(), 1, "expected one d2 block, got {:?}", blocks.len());
        assert!(blocks[0].2.contains("a -> b"));
    }

    #[test]
    fn collect_d2_blocks_ignores_non_d2_source_blocks() {
        let src = "[source,rust]\n----\nfn main() {}\n----\n";
        let blocks = collect_d2_blocks(src, "test");
        assert!(blocks.is_empty(), "rust source block should not be treated as d2");
    }

    #[test]
    fn collect_d2_blocks_handles_empty_source() {
        let blocks = collect_d2_blocks("", "test");
        assert!(blocks.is_empty());
    }

    #[test]
    fn d2_error_display_spawn() {
        let e = D2Error::Spawn(std::io::Error::other("no such binary"));
        assert!(e.to_string().contains("d2"));
    }

    #[test]
    fn d2_error_display_exit_failure() {
        let e = D2Error::ExitFailure { code: 1, index: 0, stderr: "bad input".into() };
        assert!(e.to_string().contains("status 1"));
    }

    #[test]
    fn d2_error_display_cache_write() {
        let e = D2Error::CacheWrite {
            path: "/tmp/x.svg".into(),
            source: std::io::Error::other("disk full"),
        };
        assert!(e.to_string().contains("/tmp/x.svg"));
    }

    #[test]
    fn d2_error_missing_binary_detection() {
        let e = D2Error::Spawn(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"));
        assert!(e.is_missing_binary());
    }
}
