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
            let is_plantuml = raw_block_language(rdb.attrlist())
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
mod tests {
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
    fn raw_block_language_identifies_plantuml() {
        let src = "[plantuml]\n----\nx\n----\n";
        let blocks = collect_plantuml_blocks(src, "test");
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn test_batch_render_mock_java() {
        let tmp = TempDir::new().unwrap();
        let bin_dir = tmp.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();

        let java_p = bin_dir.join("java");
        // Mock java that identifies the output dir from -o and touches files there
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&java_p, r#"#!/bin/sh
out_dir="."
while [ $# -gt 0 ]; do
  if [ "$1" = "-o" ]; then out_dir="$2"; shift; fi
  shift
done
for i in 0 1 2; do touch "${out_dir}/${i}.svg"; done
"#).unwrap();
            let mut perms = std::fs::metadata(&java_p).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&java_p, perms).unwrap();
        }

        let old_path = std::env::var_os("PATH").unwrap_or_default();
        let mut new_path = bin_dir.to_string_lossy().into_owned();
        new_path.push_str(":");
        new_path.push_str(&old_path.to_string_lossy());
        
        unsafe { std::env::set_var("PATH", new_path); }

        let diagrams = vec![
            ("A -> B".to_string(), tmp.path().join("1.svg")),
        ];
        let jar = std::path::Path::new("plantuml.jar");
        
        // We only mock for unix for now to avoid complexity with cmd/batch
        #[cfg(unix)]
        {
            let res = batch_render_plantuml(&diagrams, jar);
            assert!(res.is_ok(), "batch_render failed: {:?}", res.err());
            assert!(tmp.path().join("1.svg").exists());
        }

        unsafe { std::env::set_var("PATH", old_path); }
    }
}
