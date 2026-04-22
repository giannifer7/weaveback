# acdc renderer

`render.rs` uses `acdc` to convert `.adoc` files to HTML in-process.
`[plantuml]` blocks are pre-processed via `plantuml::preprocess_plantuml`
before parsing so that `acdc` never sees diagram blocks; a `plantuml.jar`
is sufficient and no Ruby toolchain is needed.

See link:weaveback_docgen.adoc[weaveback_docgen.adoc] for the module map.

## Constants


```rust
// <[render-exclude]>=
const EXCLUDE_DIRS: &[&str] = &["target", ".git", "node_modules", ".venv"];
// @
```


## mtime helpers


```rust
// <[render-mtime]>=
fn mtime(path: &Path) -> SystemTime {
    path.metadata()
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn theme_max_mtime(theme_dir: &Path) -> SystemTime {
    walkdir::WalkDir::new(theme_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| mtime(e.path()))
        .max()
        .unwrap_or(SystemTime::UNIX_EPOCH)
}
// @
```


## Special-char deduplication

When a `.adoc` file was processed by the weaveback macro expander, a doubled
special character (e.g. `%%` or `^^`) is the escape sequence for a literal
special character.  `acdc` knows nothing about this convention and would render
the doubled sequence verbatim.

`dedup_specials` replaces every `{s}{s}` → `{s}` for each char in `specials`
and returns the cleaned content, or `None` if no substitution was needed.


```rust
// <[render-dedup]>=
fn dedup_specials(content: &str, specials: &[char]) -> Option<String> {
    let mut out = content.to_owned();
    let mut changed = false;
    for &s in specials {
        let doubled = format!("{s}{s}");
        if out.contains(&doubled) {
            out = out.replace(&doubled, &s.to_string());
            changed = true;
        }
    }
    if changed { Some(out) } else { None }
}
// @
```


## Theme asset injection

`copy_theme_assets` copies `wb-theme.css` and `wb-theme.js` from the theme
directory to the root of `out_dir` so they are served at `/wb-theme.css` and
`/wb-theme.js`.  This is called once before the parallel render loop.

`read_docinfo` reads the small `docinfo.html` head fragment (just a `<meta>`
tag and a `<link rel="stylesheet" href="/wb-theme.css">` reference).
`inject_docinfo` splices it into the rendered HTML just before `</head>`.

`read_footer` reads `docinfo-footer.html` (a `<script src="/wb-theme.js">`
reference).  `inject_footer` splices it just before `</body>`.

Because the HTML tree may be nested (e.g.
`crates/weaveback-tangle/src/safe_writer.html`), the absolute asset paths
(`/wb-theme.css`, `/wb-theme.js`) would resolve to the domain root rather than
the site root when the site is served from a sub-path (e.g. GitHub Pages).
Before injection the absolute paths are rewritten to relative paths by
prepending the appropriate number of `../` components.


```rust
// <[render-docinfo]>=
fn copy_theme_assets(theme_dir: &Path, out_dir: &Path) {
    for name in &["wb-theme.css", "wb-theme.js"] {
        let src = theme_dir.join(name);
        let dst = out_dir.join(name);
        if src.exists() {
            std::fs::copy(&src, &dst).ok();
        }
    }
}

fn read_docinfo(theme_dir: &Path) -> Option<String> {
    let path = theme_dir.join("docinfo.html");
    std::fs::read_to_string(&path).ok()
}

fn read_footer(theme_dir: &Path) -> Option<String> {
    let path = theme_dir.join("docinfo-footer.html");
    std::fs::read_to_string(&path).ok()
}

fn inject_docinfo(mut html: String, docinfo: &str) -> String {
    if let Some(pos) = html.find("</head>") {
        html.insert_str(pos, docinfo);
    }
    html
}

fn inject_footer(mut html: String, footer: &str) -> String {
    if let Some(pos) = html.find("</body>") {
        html.insert_str(pos, footer);
    }
    html
}
// @
```


## render_docs

`render_docs` walks all `.adoc` files under `project_root`, skips those whose
HTML output is up-to-date, pre-processes stale files (plantuml + dedup), parses
with `acdc_parser`, converts with `acdc_converters_html`, injects docinfo, and
writes the result.

Rendering proceeds in two phases:

1. **Pre-scan** (sequential): for every stale `.adoc` file that contains
   plantuml blocks, collect diagram sources that are not yet in the SVG cache.
   All uncached diagrams are then passed to `plantuml::batch_render_plantuml`,
   which invokes `java -jar` exactly once regardless of diagram count.

2. **Parallel render** (rayon): each file is independently processed.
   The plantuml preprocessing step at this point only copies SVGs from the
   warm cache — no further JVM invocations.

The PlantUML SVG cache lives at `<out_dir>/../.plantuml-cache/` — a sibling of
the output directory — so that `rm -rf <out_dir>` does not invalidate it.


```rust
// <[render-entry]>=
pub fn render_docs(
    project_root: &Path,
    theme_dir: &Path,
    out_dir: &Path,
    specials: &[char],
    plantuml_jar: Option<&Path>,
    d2_theme: u32,
    d2_layout: &str,
) -> Vec<PathBuf> {
    use acdc_converters_core::Converter as _;
    use rayon::prelude::*;

    std::fs::create_dir_all(out_dir).ok();
    copy_theme_assets(theme_dir, out_dir);

    let docinfo = read_docinfo(theme_dir);
    let footer = read_footer(theme_dir);
    let theme_mtime = theme_max_mtime(theme_dir);
    let adoc_files = find_adoc_files(project_root);

    // SVG cache lives outside out_dir so `rm -rf <out_dir>` doesn't blow it away.
    let svg_cache_dir = out_dir.parent().unwrap_or(out_dir).join(".plantuml-cache");
    std::fs::create_dir_all(&svg_cache_dir).ok();

    // Phase 1: collect uncached PlantUML diagrams from all stale files, then
    // batch-render them in a single JVM invocation.
    if let Some(jar) = plantuml_jar {
        let mut to_render: Vec<(String, PathBuf)> = Vec::new();
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

        for adoc in &adoc_files {
            let rel = adoc.strip_prefix(project_root).unwrap_or(adoc);
            let out_file = out_dir.join(rel).with_extension("html");
            if out_file.exists()
                && mtime(&out_file) >= mtime(adoc)
                && mtime(&out_file) >= theme_mtime
            {
                continue;
            }
            let source = std::fs::read_to_string(adoc).unwrap_or_default();
            let label = adoc.strip_prefix(project_root).unwrap_or(adoc).to_string_lossy();
            for (diagram_src, cache_path) in crate::plantuml::collect_uncached_plantuml_diagrams(
                &source, &svg_cache_dir, &label,
            ) {
                if seen.insert(cache_path.clone()) {
                    to_render.push((diagram_src, cache_path));
                }
            }
        }

        if !to_render.is_empty()
            && let Err(e) = crate::plantuml::batch_render_plantuml(&to_render, jar) {
                eprintln!("plantuml batch: {e}");
                std::process::exit(1);
        }
    }

    // Phase 2: parallel acdc render; plantuml preprocessing only copies from cache.
    let results: Vec<(PathBuf, bool)> = adoc_files
        .par_iter()
        .map(|adoc| {
            let rel = adoc.strip_prefix(project_root).unwrap_or(adoc);
            let out_file = out_dir.join(rel).with_extension("html");
            std::fs::create_dir_all(out_file.parent().unwrap()).ok();

            if out_file.exists()
                && mtime(&out_file) >= mtime(adoc)
                && mtime(&out_file) >= theme_mtime
            {
                return (out_file, false);
            }

            let source = std::fs::read_to_string(adoc).unwrap_or_default();

            // 1. PlantUML pre-processing.
            let after_plantuml: Option<String> = if let Some(jar) = plantuml_jar {
                let images_dir = out_file.parent().unwrap_or(out_dir);
                let label = adoc.strip_prefix(project_root).unwrap_or(adoc).to_string_lossy();
                match crate::plantuml::preprocess_plantuml(
                    &source, jar, images_dir, &svg_cache_dir, &label,
                ) {
                    Ok(opt) => opt,
                    Err(e) => {
                        eprintln!("plantuml: {}: {}", adoc.display(), e);
                        std::process::exit(1);
                    }
                }
            } else {
                None
            };

            // 1.5. D2 pre-processing.
            let base_before_d2 = after_plantuml.as_deref().unwrap_or(&source);
            let after_d2: Option<String> = {
                let images_dir = out_file.parent().unwrap_or(out_dir);
                let label = adoc.strip_prefix(project_root).unwrap_or(adoc).to_string_lossy();
                match crate::d2::preprocess_d2(
                    base_before_d2, images_dir, &svg_cache_dir, &label, d2_theme, d2_layout,
                ) {
                    Ok(opt) => opt,
                    Err(e) => {
                        eprintln!("d2: {}: {}", adoc.display(), e);
                        std::process::exit(1);
                    }
                }
            };

            // 2. Special-char deduplication.
            let base = after_d2.as_deref().or(after_plantuml.as_deref()).unwrap_or(&source);
            let processed: String = dedup_specials(base, specials)
                .unwrap_or_else(|| base.to_owned());

            // 3. Build per-file acdc options.
            let source_dir = adoc.parent().unwrap_or(project_root);
            let docname = adoc
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_owned();
            let images_dir = out_file
                .parent()
                .unwrap_or(out_dir)
                .to_string_lossy()
                .into_owned();

            let parse_options = acdc_parser::Options::builder()
                .with_attribute("imagesdir", images_dir)
                .with_attribute("source-highlighter", "syntect")
                .with_attribute("syntect-css", "class")
                .build();

            // 4. Parse — catch panics from parser bugs in experimental acdc.
            let parse_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                acdc_parser::parse(&processed, &parse_options)
            }));
            let doc = match parse_result {
                Ok(Ok(d)) => d,
                Ok(Err(e)) => {
                    eprintln!("acdc parse: {}: {}", adoc.display(), e);
                    return (out_file, false);
                }
                Err(_) => {
                    eprintln!("acdc parse: {}: parser panicked, skipping", adoc.display());
                    return (out_file, false);
                }
            };

            // 5. Render to HTML — catch panics from renderer bugs in experimental acdc.
            let conv_options = acdc_converters_core::Options::builder().build();
            let processor = acdc_converters_html::Processor::new(
                conv_options,
                doc.attributes.clone(),
            );
            let render_opts = acdc_converters_html::RenderOptions {
                embedded: false,
                source_dir: Some(source_dir.to_path_buf()),
                docname: Some(docname),
                ..Default::default()
            };
            let render_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                processor.convert_to_string(&doc, &render_opts)
            }));
            let mut html = match render_result {
                Ok(Ok(h)) => h,
                Ok(Err(e)) => {
                    eprintln!("acdc render: {}: {}", adoc.display(), e);
                    return (out_file, false);
                }
                Err(_) => {
                    eprintln!("acdc render: {}: renderer panicked, skipping", adoc.display());
                    return (out_file, false);
                }
            };

            // 6. Inject head fragment (link tag) and footer script tag.
            // Rewrite absolute asset paths (href="/…", src="/…") to relative
            // so the page works when served from a subdirectory (e.g. GitHub Pages).
            let depth = out_file
                .strip_prefix(out_dir)
                .map(|rel| rel.components().count().saturating_sub(1))
                .unwrap_or(0);
            let prefix = "../".repeat(depth);
            if let Some(ref di) = docinfo {
                let patched = di
                    .replace("href=\"/", &format!("href=\"{prefix}"))
                    .replace("src=\"/", &format!("src=\"{prefix}"));
                html = inject_docinfo(html, &patched);
            }
            if let Some(ref f) = footer {
                let patched = f
                    .replace("href=\"/", &format!("href=\"{prefix}"))
                    .replace("src=\"/", &format!("src=\"{prefix}"));
                html = inject_footer(html, &patched);
            }

            if let Err(e) = std::fs::write(&out_file, &html) {
                eprintln!("write {}: {}", out_file.display(), e);
                std::process::exit(1);
            }

            (out_file, true)
        })
        .collect();

    let all_html: Vec<PathBuf> = results.iter().map(|(p, _)| p.clone()).collect();
    let rendered = results.iter().filter(|(_, r)| *r).count();

    if rendered == 0 {
        println!("docs: nothing to do");
    } else {
        println!("docs: rendered {rendered} file(s)");
    }

    all_html
}
// @
```


## File discovery

`find_adoc_files` walks the project tree, skipping `EXCLUDE_DIRS`, and returns
a sorted list of all `.adoc` paths.


```rust
// <[render-discover]>=
fn find_adoc_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                !EXCLUDE_DIRS.iter().any(|ex| name == *ex)
            } else {
                true
            }
        })
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "adoc")
        })
        .map(|e| e.into_path())
        .collect();
    files.sort();
    files
}
// @
```


## Assembly


```rust
// <[@file weaveback-docgen/src/render.rs]>=
// weaveback-docgen/src/render.rs
// I'd Really Rather You Didn't edit this generated file.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

// <[render-exclude]>
// <[render-mtime]>
// <[render-dedup]>
// <[render-docinfo]>
// <[render-entry]>
// <[render-discover]>
#[cfg(test)]
mod tests;

// @
```


## Tests

The test body is generated as `render/tests.rs` and linked from
`render.rs` with `#[cfg(test)] mod tests;`.


```rust
// <[@file weaveback-docgen/src/render/tests.rs]>=
// weaveback-docgen/src/render/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn dedup_specials_only_rewrites_doubled_configured_sigils() {
    assert_eq!(
        dedup_specials("100%% ready ^^", &['%', '^']),
        Some("100% ready ^".to_string())
    );
    assert_eq!(dedup_specials("100% ready", &['%', '^']), None);
    assert_eq!(dedup_specials("a##b", &['%']), None);
}

#[test]
fn inject_helpers_insert_only_when_expected_markers_exist() {
    let html = "<html><head></head><body>Hello</body></html>".to_string();
    assert_eq!(
        inject_docinfo(html.clone(), "<meta name=\"x\" />"),
        "<html><head><meta name=\"x\" /></head><body>Hello</body></html>"
    );
    assert_eq!(
        inject_footer(html.clone(), "<footer>F</footer>"),
        "<html><head></head><body>Hello<footer>F</footer></body></html>"
    );
    assert_eq!(inject_docinfo("<html></html>".to_string(), "x"), "<html></html>");
    assert_eq!(inject_footer("<html></html>".to_string(), "x"), "<html></html>");
}

#[test]
fn theme_helpers_copy_assets_and_read_optional_html() {
    let dir = tempdir().expect("tempdir");
    let theme = dir.path().join("theme");
    let out = dir.path().join("out");
    fs::create_dir_all(&theme).expect("theme dir");
    fs::create_dir_all(&out).expect("out dir");

    fs::write(theme.join("wb-theme.css"), "body{}").expect("css");
    fs::write(theme.join("wb-theme.js"), "console.log(1);").expect("js");
    fs::write(theme.join("docinfo.html"), "<meta>").expect("docinfo");
    fs::write(theme.join("docinfo-footer.html"), "<footer>").expect("footer");

    copy_theme_assets(&theme, &out);
    assert_eq!(fs::read_to_string(out.join("wb-theme.css")).expect("read css"), "body{}");
    assert_eq!(fs::read_to_string(out.join("wb-theme.js")).expect("read js"), "console.log(1);");
    assert_eq!(read_docinfo(&theme).as_deref(), Some("<meta>"));
    assert_eq!(read_footer(&theme).as_deref(), Some("<footer>"));
    assert!(theme_max_mtime(&theme) >= SystemTime::UNIX_EPOCH);
}

#[test]
fn find_adoc_files_respects_excluded_directories() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("docs")).expect("docs dir");
    fs::create_dir_all(dir.path().join("target")).expect("target dir");
    fs::write(dir.path().join("docs").join("guide.adoc"), "= Guide\n").expect("guide");
    fs::write(dir.path().join("target").join("generated.adoc"), "= Skip\n").expect("generated");

    let files = find_adoc_files(dir.path());
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("docs/guide.adoc"));
}

#[test]
fn render_docs_renders_simple_page_and_copies_theme_assets() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join("project");
    let theme = root.join("scripts/asciidoc-theme");
    let out = root.join("docs/html");
    fs::create_dir_all(root.join("docs")).expect("docs dir");
    fs::create_dir_all(&theme).expect("theme dir");
    fs::write(root.join("docs/index.adoc"), "= Hello\n\n100%% ready.\n").expect("adoc");
    fs::write(theme.join("wb-theme.css"), "body{}").expect("css");
    fs::write(theme.join("wb-theme.js"), "console.log(1);").expect("js");
    fs::write(theme.join("docinfo.html"), "<meta name=\"x\" />").expect("docinfo");
    fs::write(theme.join("docinfo-footer.html"), "<footer>F</footer>").expect("footer");

    let rendered = render_docs(&root, &theme, &out, &['%'], None, 200, "elk");
    assert_eq!(rendered, vec![out.join("docs/index.html")]);

    let html = fs::read_to_string(out.join("docs/index.html")).expect("html");
    assert!(html.contains("Hello"));
    assert!(html.contains("100% ready."));
    assert!(html.contains("<meta name=\"x\" />"));
    assert!(html.contains("<footer>F</footer>"));
    assert_eq!(fs::read_to_string(out.join("wb-theme.css")).expect("out css"), "body{}");
    assert_eq!(fs::read_to_string(out.join("wb-theme.js")).expect("out js"), "console.log(1);");
}

#[test]
fn render_docs_skips_up_to_date_outputs() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join("project");
    let theme = root.join("scripts/asciidoc-theme");
    let out = root.join("docs/html");
    fs::create_dir_all(root.join("docs")).expect("docs dir");
    fs::create_dir_all(out.join("docs")).expect("out docs dir");
    fs::create_dir_all(&theme).expect("theme dir");
    fs::write(root.join("docs/index.adoc"), "= Hello\n").expect("adoc");
    fs::write(theme.join("wb-theme.css"), "body{}").expect("css");
    fs::write(theme.join("wb-theme.js"), "console.log(1);").expect("js");
    fs::write(out.join("docs/index.html"), "<html>cached</html>").expect("html");

    let rendered = render_docs(&root, &theme, &out, &[], None, 200, "elk");
    assert_eq!(rendered, vec![out.join("docs/index.html")]);
    assert_eq!(fs::read_to_string(out.join("docs/index.html")).expect("html"), "<html>cached</html>");
}

// @
```

