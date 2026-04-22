// weaveback-docgen/src/d2.rs
// I'd Really Rather You Didn't edit this generated file.

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
fn collect_d2_blocks(source: &str, label: &str) -> Vec<(usize, usize, String)> {
    crate::adoc_scan::collect_listing_blocks_by_language(source, "d2", label)
        .into_iter()
        .map(|block| (block.start, block.end, block.content))
        .collect()
}
pub fn render_d2_diagram(
    diagram_source: &str,
    index: usize,
    theme: u32,
    layout: &str,
) -> Result<Vec<u8>, D2Error> {
    let d2_bin = std::env::var("WEAVEBACK_D2_BIN").unwrap_or_else(|_| "d2".to_string());
    let mut child = std::process::Command::new(&d2_bin)
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
            let svg_bytes = render_d2_diagram(&diagram_src, index, theme, layout)?;
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
mod tests;

