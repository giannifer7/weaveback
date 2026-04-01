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
    use asciidoc_parser::{Parser, blocks::IsBlock};

    // asciidoc-parser is experimental and may panic on some input.
    // Treat a panic as "no plantuml blocks found" so the file is still processed.
    let parse_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Parser::default().parse(source)
    }));
    let doc = match parse_result {
        Ok(d) => d,
        Err(_) => {
            eprintln!("plantuml: {label}: asciidoc-parser panicked while scanning for plantuml blocks — skipping plantuml pre-processing for this file");
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
    use asciidoc_parser::{HasSpan, blocks::{Block, IsBlock}};

    for block in blocks {
        if let Block::RawDelimited(rdb) = block {
            let is_plantuml = rdb
                .attrlist()
                .and_then(|a| a.block_style())
                .map(|s| s == "plantuml")
                .unwrap_or(false);
            if is_plantuml {
                let span = rdb.span();
                let start = span.byte_offset();
                let end = start + span.data().len();
                let diagram_src = rdb.content().original().data().to_owned();
                out.push((start, end, diagram_src));
                continue; // no nested blocks inside a raw delimited block
            }
        }
        collect_from_blocks(block.nested_blocks(), out);
    }
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
        .args(["-jar", &jar_str, "-tsvg", "-pipe"])
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

    Ok(output.stdout)
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
    cmd.args(["-jar", &jar_str, "-tsvg", "-o", &out_str]);
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
        }

        // Copy from cache to output dir if not already there.
        if !svg_out_path.exists() {
            std::fs::copy(&svg_cache_path, &svg_out_path).map_err(|e| PlantUmlError::CacheWrite {
                path: svg_out_path.to_string_lossy().into_owned(),
                source: e,
            })?;
        }

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
