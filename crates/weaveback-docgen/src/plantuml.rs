// weaveback-docgen/src/plantuml.rs
// I'd Really Rather You Didn't edit this generated file.

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum PlantUmlError {
    #[error("failed to spawn PlantUML JAR '{jar}': {source}")]
    Spawn { jar: String, source: std::io::Error },

    #[error("failed to write to PlantUML stdin: {0}")]
    Stdin(std::io::Error),

    #[error("failed to read PlantUML stdout: {0}")]
    Stdout(std::io::Error),

    #[error("PlantUML exited with status {code} for diagram #{index}")]
    ExitFailure { code: i32, index: usize },

    #[error("failed to write SVG cache file '{path}': {source}")]
    CacheWrite { path: String, source: std::io::Error },

    #[error("PlantUML batch render failed with status {code}")]
    BatchFailed { code: i32 },
}
fn collect_plantuml_blocks(source: &str, label: &str) -> Vec<(usize, usize, String)> {
    crate::adoc_scan::collect_listing_blocks_by_language(source, "plantuml", label)
        .into_iter()
        .map(|block| (block.start, block.end, block.content))
        .collect()
}
fn render_diagram(
    jar: &Path,
    diagram_source: &str,
    index: usize,
) -> Result<Vec<u8>, PlantUmlError> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let jar_str = jar.to_string_lossy().into_owned();

    let mut child = Command::new("java")
        .args(["-Djava.awt.headless=true", "-jar", &jar_str, "-tsvg", "-pipe"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| PlantUmlError::Spawn { jar: jar_str.clone(), source: e })?;

    child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(diagram_source.as_bytes())
        .map_err(PlantUmlError::Stdin)?;

    let output = child.wait_with_output().map_err(PlantUmlError::Stdout)?;

    if !output.status.success() {
        return Err(PlantUmlError::ExitFailure {
            code: output.status.code().unwrap_or(-1),
            index,
        });
    }

    Ok(normalize_svg_background(output.stdout))
}

fn normalize_svg_background(svg: Vec<u8>) -> Vec<u8> {
    let Ok(text) = String::from_utf8(svg.clone()) else {
        return svg;
    };
    text.replace("background:#FFFFFF;", "background:transparent;")
        .replace("background:#ffffff;", "background:transparent;")
        .into_bytes()
}

fn normalize_svg_file_in_place(path: &Path) -> Result<(), PlantUmlError> {
    let bytes = std::fs::read(path).map_err(|e| PlantUmlError::CacheWrite {
        path: path.to_string_lossy().into_owned(),
        source: e,
    })?;
    let normalized = normalize_svg_background(bytes);
    std::fs::write(path, normalized).map_err(|e| PlantUmlError::CacheWrite {
        path: path.to_string_lossy().into_owned(),
        source: e,
    })?;
    Ok(())
}
pub(crate) fn collect_uncached_plantuml_diagrams(
    source: &str,
    svg_cache_dir: &Path,
    label: &str,
) -> Vec<(String, std::path::PathBuf)> {
    collect_plantuml_blocks(source, label)
        .into_iter()
        .filter_map(|(_, _, diagram_src)| {
            let hash = blake3::hash(diagram_src.as_bytes());
            let svg_name = format!("{}.svg", hash.to_hex());
            let cache_path = svg_cache_dir.join(svg_name);
            if !cache_path.exists() {
                Some((diagram_src, cache_path))
            } else {
                None
            }
        })
        .collect()
}
pub(crate) fn batch_render_plantuml(
    diagrams: &[(String, std::path::PathBuf)],
    jar: &Path,
) -> Result<(), PlantUmlError> {
    if diagrams.is_empty() {
        return Ok(());
    }

    let tmp_dir = tempfile::tempdir()
        .map_err(|e| PlantUmlError::Spawn { jar: jar.to_string_lossy().into_owned(), source: e })?;

    for (i, (source, _)) in diagrams.iter().enumerate() {
        let puml = tmp_dir.path().join(format!("{i}.puml"));
        std::fs::write(&puml, source.as_bytes())
            .map_err(|e| PlantUmlError::CacheWrite {
                path: puml.to_string_lossy().into_owned(),
                source: e,
            })?;
    }

    let jar_str = jar.to_string_lossy().into_owned();
    let out_str = tmp_dir.path().to_string_lossy().into_owned();

    let mut cmd = std::process::Command::new("java");
    cmd.args(["-Djava.awt.headless=true", "-jar", &jar_str, "-tsvg", "-o", &out_str]);
    for i in 0..diagrams.len() {
        cmd.arg(tmp_dir.path().join(format!("{i}.puml")));
    }

    let status = cmd
        .status()
        .map_err(|e| PlantUmlError::Spawn { jar: jar_str, source: e })?;

    if !status.success() {
        return Err(PlantUmlError::BatchFailed { code: status.code().unwrap_or(-1) });
    }

    for (i, (_, cache_path)) in diagrams.iter().enumerate() {
        let svg = tmp_dir.path().join(format!("{i}.svg"));
        std::fs::copy(&svg, cache_path)
            .map_err(|e| PlantUmlError::CacheWrite {
                path: cache_path.to_string_lossy().into_owned(),
                source: e,
            })?;
    }

    Ok(())
}
pub fn preprocess_plantuml(
    source: &str,
    jar: &Path,
    images_out_dir: &Path,
    svg_cache_dir: &Path,
    label: &str,
) -> Result<Option<String>, PlantUmlError> {
    let blocks = collect_plantuml_blocks(source, label);
    if blocks.is_empty() {
        return Ok(None);
    }

    std::fs::create_dir_all(images_out_dir).ok();

    // Collect (start, end, replacement_string) for each block.
    let mut replacements: Vec<(usize, usize, String)> = Vec::with_capacity(blocks.len());

    for (index, (start, end, diagram_src)) in blocks.into_iter().enumerate() {
        let hash = blake3::hash(diagram_src.as_bytes());
        let svg_name = format!("{}.svg", hash.to_hex());
        let svg_cache_path = svg_cache_dir.join(&svg_name);
        let svg_out_path = images_out_dir.join(&svg_name);

        // Render to persistent cache if not already there.
        if !svg_cache_path.exists() {
            let svg_bytes = render_diagram(jar, &diagram_src, index)?;
            std::fs::write(&svg_cache_path, &svg_bytes).map_err(|e| PlantUmlError::CacheWrite {
                path: svg_cache_path.to_string_lossy().into_owned(),
                source: e,
            })?;
        } else {
            normalize_svg_file_in_place(&svg_cache_path)?;
        }

        // Copy from cache to output dir on every render so stale local outputs are refreshed.
        std::fs::copy(&svg_cache_path, &svg_out_path).map_err(|e| PlantUmlError::CacheWrite {
            path: svg_out_path.to_string_lossy().into_owned(),
            source: e,
        })?;

        let replacement = format!("image::{svg_name}[PlantUML diagram]\n");
        replacements.push((start, end, replacement));
    }

    // Apply in reverse order so earlier offsets stay valid.
    let mut result = source.to_owned();
    for (start, end, replacement) in replacements.into_iter().rev() {
        result.replace_range(start..end, &replacement);
    }

    Ok(Some(result))
}
#[cfg(test)]
mod tests;

