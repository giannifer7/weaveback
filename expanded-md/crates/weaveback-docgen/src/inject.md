# HTML post-processing

`inject.rs` performs two post-processing passes on the generated HTML tree.

`rewrite_adoc_links` rewrites every `href="….adoc…"` to `href="….html…"`.
AsciiDoc cross-document links use `.adoc` extensions by convention, but the
rendered HTML files use `.html`, so a simple regex substitution is needed
after the acdc render.

`inject_xref` embeds a `window.__xref` JSON object in the `<head>` of every
HTML page that has a matching cross-reference entry.  The object is consumed
by the side-panel JavaScript in the shared docinfo template.

See link:xref.adoc[xref.adoc] for how the graph is built and
link:weaveback_docgen.adoc[weaveback_docgen.adoc] for the module map.

## .adoc → .html link rewriting

The regex matches `href="…something.adoc…"` and replaces the `.adoc`
extension.  It runs over all HTML files under `out_dir`.


```rust
// <[inject-rewrite]>=
fn adoc_href_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(href="[^"]*?)\.adoc([^"]*?")"#).unwrap())
}

pub fn rewrite_adoc_links(out_dir: &Path) {
    let re = adoc_href_re();
    let html_files: Vec<_> = walkdir::WalkDir::new(out_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "html"))
        .map(|e| e.into_path())
        .collect();

    for html_file in html_files {
        let Ok(content) = std::fs::read_to_string(&html_file) else {
            continue;
        };
        if !content.contains(".adoc") {
            continue;
        }
        let patched = re.replace_all(&content, r#"$1.html$2"#);
        if patched != content {
            let _ = std::fs::write(&html_file, patched.as_ref());
        }
    }
}
// @
```


## xref injection

`inject_xref` runs two passes.  The first pass matches each HTML file against
the xref graph by module key (derived from the path relative to `out_dir`).
The second pass handles adoc-derived pages where the `.adoc` filename differs
from the generated `.rs` filename (e.g. `cli.adoc` generates
`weaveback-macro.rs`); these have entries in `adoc_map` built by
`xref::scan_adoc_file_declarations`.

`existing_html` is threaded through so links to unrendered modules (modules
that have `.rs` but no `.adoc` literate source) are filtered out of the
injected JSON.


```rust
// <[inject-xref]>=
pub fn inject_xref(
    out_dir: &Path,
    xref: &HashMap<String, XrefEntry>,
    existing_html: &HashSet<String>,
    adoc_map: &HashMap<String, Vec<String>>,
) {
    let mut html_to_key: HashMap<String, &str> = HashMap::new();
    for key in xref.keys() {
        html_to_key.insert(html_path_for_key(key), key.as_str());
    }

    let html_files: Vec<_> = walkdir::WalkDir::new(out_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "html"))
        .map(|e| e.into_path())
        .collect();

    for html_file in &html_files {
        let rel = html_file
            .strip_prefix(out_dir)
            .ok()
            .and_then(|r| r.to_str())
            .map(|s| s.replace('\\', "/"));
        let Some(rel_str) = rel else { continue };
        let Some(&key) = html_to_key.get(&rel_str) else {
            continue;
        };
        let Some(entry) = xref.get(key) else { continue };
        do_inject(html_file, entry, existing_html);
    }

    for (adoc_html, keys) in adoc_map {
        if !existing_html.contains(adoc_html) {
            continue;
        }
        // Skip if first pass already injected xref (unlikely but safe)
        if html_to_key.contains_key(adoc_html) {
            continue;
        }
        let entries: Vec<&XrefEntry> = keys.iter().filter_map(|k| xref.get(k.as_str())).collect();
        if entries.is_empty() {
            continue;
        }
        let merged = merge_xref_entries(adoc_html, &entries);
        let html_file = out_dir.join(std::path::Path::new(adoc_html));
        do_inject(&html_file, &merged, existing_html);
    }
}
// @
```


## Merging multiple xref entries

When a single `.adoc` file declares multiple `@file` chunks (e.g. a tests
adoc that generates several `.rs` files), the xref entries for all those
modules are merged: imports, `imported_by`, and symbols are concatenated,
sorted, and de-duplicated.


```rust
// <[inject-merge]>=
fn merge_xref_entries(html: &str, entries: &[&XrefEntry]) -> XrefEntry {
    let mut imports: Vec<XrefLink> = entries.iter().flat_map(|e| e.imports.iter().cloned()).collect();
    imports.sort_by(|a, b| a.key.cmp(&b.key));
    imports.dedup_by(|a, b| a.key == b.key);

    let mut imported_by: Vec<XrefLink> = entries.iter().flat_map(|e| e.imported_by.iter().cloned()).collect();
    imported_by.sort_by(|a, b| a.key.cmp(&b.key));
    imported_by.dedup_by(|a, b| a.key == b.key);

    let mut symbols: Vec<String> = entries.iter().flat_map(|e| e.symbols.iter().cloned()).collect();
    symbols.sort();
    symbols.dedup();

    let mut lsp_links: Vec<XrefLink> = entries.iter().flat_map(|e| e.lsp_links.iter().cloned()).collect();
    lsp_links.sort_by(|a, b| a.key.cmp(&b.key));
    lsp_links.dedup_by(|a, b| a.key == b.key);

    XrefEntry {
        html: html.to_string(),
        imports,
        imported_by,
        symbols,
        lsp_links,
    }
}
// @
```


## do_inject

`do_inject` first strips any previously injected `window.__xref` script so
re-runs are idempotent, then builds a compact xref object filtered to pages
that were actually rendered, and inserts it as a `<script>` tag just before
`</head>`.


```rust
// <[inject-do]>=
fn do_inject(html_file: &Path, entry: &XrefEntry, existing_html: &HashSet<String>) {
    let content = match std::fs::read_to_string(html_file) {
        Ok(c) => c,
        Err(_) => return,
    };
    if !content.contains("</head>") {
        return;
    }

    // Remove ALL previously injected xref scripts so re-runs stay idempotent.
    let mut content = content;
    while let Some(start) = content.find("<script>window.__xref=") {
        let Some(rel_end) = content[start..].find("</script>") else { break };
        let end = start + rel_end + "</script>".len();
        let end = if content.as_bytes().get(end) == Some(&b'\n') { end + 1 } else { end };
        content = format!("{}{}", &content[..start], &content[end..]);
    }

    // Build a compact per-page xref object, filtering out links whose target
    // HTML page was not rendered (module has .rs but no .adoc literate source).
    let imports: Vec<_> = entry.imports.iter()
        .filter(|l| existing_html.contains(&l.html))
        .collect();
    let imported_by: Vec<_> = entry.imported_by.iter()
        .filter(|l| existing_html.contains(&l.html))
        .collect();

    let xref_obj = serde_json::json!({
        "self":       entry.html,
        "imports":    imports,
        "importedBy": imported_by,
        "symbols":    entry.symbols,
    });

    let tag = format!(
        "<script>window.__xref={}</script>\n",
        xref_obj
    );
    let patched = content.replacen("</head>", &format!("{}</head>", tag), 1);
    let _ = std::fs::write(html_file, patched);
}
// @
```


## Chunk ID annotation

`inject_chunk_ids` adds `data-chunk-id="file|name|nth"` attributes to
`<div class="listingblock">` elements whose `<code>` block opens with a
weaveback chunk-open marker.  This enables the browser inline editor to
identify editable chunks without server round-trips.

`chunk_open_re` matches both `&lt;[name]&gt;=` and `&lt;&lt;name&gt;&gt;=`
style markers (HTML-encoded `<[...]>=` and `<<...>>=`).  Captures from either
group feed the name extraction below.

After extracting the raw name string, `@replace` and `@reversed` modifier
prefixes are stripped.  Names beginning with `@file ` are skipped (file chunks
are assembly roots, not directly editable).  `nth` is a 0-based counter per
chunk name within each HTML file, matching the storage convention in
`chunk_defs`.

The pass is idempotent: existing `data-chunk-id` attributes are stripped at
the start of each file's processing.


```rust
// <[inject-chunk-ids]>=
fn chunk_open_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"&lt;(?:\[([^\]]+)\]&gt;|&lt;([^&<>]+)&gt;&gt;)=").unwrap()
    })
}

fn chunk_id_strip_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#" data-chunk-id="[^"]*""#).unwrap()
    })
}

fn annotate_chunk_ids(html: &str, adoc_file: &str, re: &Regex) -> String {
    let html_cow = chunk_id_strip_re().replace_all(html, "");
    let html: &str = html_cow.as_ref();

    let marker = r#"<div class="listingblock">"#;
    let mut nth_map: HashMap<String, u32> = HashMap::new();
    let mut result = String::with_capacity(html.len() + 512);
    let mut pos = 0;

    while let Some(rel_start) = html[pos..].find(marker) {
        let abs_start = pos + rel_start;
        result.push_str(&html[pos..abs_start]);

        let after_div = &html[abs_start + marker.len()..];
        let chunk_id = (|| -> Option<String> {
            let code_start = after_div.find("<code")?;
            let after_code = &after_div[code_start..];
            let tag_end = after_code.find('>')?;
            let code_content = &after_code[tag_end + 1..];
            let raw = code_content.len().min(400);
            let end = (0..=raw).rev().find(|&i| code_content.is_char_boundary(i)).unwrap_or(0);
            let search_zone = &code_content[..end];
            let cap = re.captures(search_zone)?;
            let raw = cap.get(1).or_else(|| cap.get(2))?.as_str().trim();
            let name = raw
                .strip_prefix("@replace")
                .map(|s| s.trim_start())
                .unwrap_or(raw);
            let name = name
                .strip_prefix("@reversed")
                .map(|s| s.trim_start())
                .unwrap_or(name);
            if name.starts_with("@file ") || name.is_empty() {
                return None;
            }
            let nth = *nth_map.get(name).unwrap_or(&0);
            nth_map.insert(name.to_string(), nth + 1);
            Some(format!("{adoc_file}|{name}|{nth}"))
        })();

        if let Some(id) = chunk_id {
            result.push_str(&format!(r#"<div class="listingblock" id="{id}" data-chunk-id="{id}">"#));
        } else {
            result.push_str(marker);
        }
        pos = abs_start + marker.len();
    }
    result.push_str(&html[pos..]);
    result
}

pub fn inject_chunk_ids(out_dir: &Path) {
    let re = chunk_open_re();
    let html_files: Vec<_> = walkdir::WalkDir::new(out_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "html"))
        .map(|e| e.into_path())
        .collect();

    for html_file in html_files {
        let rel = match html_file.strip_prefix(out_dir) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        let adoc_rel = rel.replace(".html", ".adoc");

        let Ok(content) = std::fs::read_to_string(&html_file) else { continue };
        if !content.contains("listingblock") { continue }

        let patched = annotate_chunk_ids(&content, &adoc_rel, re);
        if patched != content {
            let _ = std::fs::write(&html_file, &patched);
        }
    }
}
// @
```


## Tests

The unit tests belong to the same source of truth as the production logic, but
they generate to a separate Rust module.  This keeps the literate explanation
and tests linked while avoiding another large inline `mod tests` block in the
generated production file.

The test body is generated as `inject/tests.rs` and linked from
`inject.rs` with `#[cfg(test)] mod tests;`.


```rust
// <[@file weaveback-docgen/src/inject/tests.rs]>=
// weaveback-docgen/src/inject/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use std::fs;
use tempfile::tempdir;

fn link(key: &str, html: &str) -> XrefLink {
    XrefLink {
        key: key.to_string(),
        label: key.to_string(),
        html: html.to_string(),
    }
}

#[test]
fn rewrite_adoc_links_updates_html_files_only() {
    let dir = tempdir().expect("tempdir");
    let html = dir.path().join("index.html");
    let txt = dir.path().join("plain.txt");
    fs::write(&html, r#"<a href="guide.adoc#intro">Guide</a>"#).expect("html");
    fs::write(&txt, "guide.adoc").expect("txt");

    rewrite_adoc_links(dir.path());

    assert_eq!(
        fs::read_to_string(&html).expect("read html"),
        r#"<a href="guide.html#intro">Guide</a>"#
    );
    assert_eq!(fs::read_to_string(&txt).expect("read txt"), "guide.adoc");
}

#[test]
fn merge_xref_entries_deduplicates_links_and_symbols() {
    let left = XrefEntry {
        html: "a.html".to_string(),
        imports: vec![link("dep/a", "dep/a.html"), link("dep/a", "dep/a.html")],
        imported_by: vec![link("user/a", "user/a.html")],
        symbols: vec!["Foo".to_string(), "Bar".to_string()],
        lsp_links: vec![link("lsp/a", "lsp/a.html")],
    };
    let right = XrefEntry {
        html: "a.html".to_string(),
        imports: vec![link("dep/b", "dep/b.html")],
        imported_by: vec![link("user/a", "user/a.html"), link("user/b", "user/b.html")],
        symbols: vec!["Bar".to_string(), "Baz".to_string()],
        lsp_links: vec![link("lsp/a", "lsp/a.html"), link("lsp/b", "lsp/b.html")],
    };

    let merged = merge_xref_entries("page.html", &[&left, &right]);
    assert_eq!(merged.html, "page.html");
    assert_eq!(merged.imports.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(), vec!["dep/a", "dep/b"]);
    assert_eq!(merged.imported_by.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(), vec!["user/a", "user/b"]);
    assert_eq!(merged.symbols, vec!["Bar".to_string(), "Baz".to_string(), "Foo".to_string()]);
    assert_eq!(merged.lsp_links.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(), vec!["lsp/a", "lsp/b"]);
}

#[test]
fn annotate_chunk_ids_marks_non_file_chunks_and_replaces_old_ids() {
    let html = concat!(
        r#"<div class="listingblock" data-chunk-id="old">"#,
        r#"<div class="content"><pre><code>// &lt;[alpha]&gt;="#,
        "</code></pre></div></div>",
        r#"<div class="listingblock"><div class="content"><pre><code>// &lt;[alpha]&gt;="#,
        "</code></pre></div></div>",
        r#"<div class="listingblock"><div class="content"><pre><code>// &lt;[@file out.rs]&gt;="#,
        "</code></pre></div></div>"
    );

    let patched = annotate_chunk_ids(html, "docs/page.adoc", chunk_open_re());
    assert!(patched.contains(r#"id="docs/page.adoc|alpha|0""#));
    assert!(patched.contains(r#"id="docs/page.adoc|alpha|1""#));
    assert!(!patched.contains("old"));
    assert!(!patched.contains("@file out.rs|0"));
}

#[test]
fn do_inject_replaces_old_xref_script_and_filters_missing_targets() {
    let dir = tempdir().expect("tempdir");
    let html_file = dir.path().join("page.html");
    fs::write(
        &html_file,
        concat!(
            "<html><head>",
            "<script>window.__xref={\"stale\":true}</script>\n",
            "</head><body>Body</body></html>"
        ),
    )
    .expect("html");

    let entry = XrefEntry {
        html: "page.html".to_string(),
        imports: vec![link("dep/kept", "dep/kept.html"), link("dep/dropped", "dep/dropped.html")],
        imported_by: vec![link("user/kept", "user/kept.html"), link("user/dropped", "user/dropped.html")],
        symbols: vec!["Demo".to_string()],
        lsp_links: Vec::new(),
    };
    let existing = HashSet::from([
        "page.html".to_string(),
        "dep/kept.html".to_string(),
        "user/kept.html".to_string(),
    ]);

    do_inject(&html_file, &entry, &existing);
    let content = fs::read_to_string(&html_file).expect("read html");
    assert_eq!(content.matches("window.__xref=").count(), 1);
    assert!(content.contains("dep/kept.html"));
    assert!(content.contains("user/kept.html"));
    assert!(!content.contains("dep/dropped.html"));
    assert!(!content.contains("user/dropped.html"));
    assert!(!content.contains("\"stale\":true"));
}

#[test]
fn inject_xref_handles_direct_html_and_adoc_override_maps() {
    let dir = tempdir().expect("tempdir");
    let out = dir.path();
    fs::create_dir_all(out.join("crates/demo/src")).expect("mkdir");
    fs::create_dir_all(out.join("docs")).expect("docs mkdir");
    fs::write(out.join("crates/demo/src/mod.html"), "<html><head></head><body></body></html>").expect("mod html");
    fs::write(out.join("docs/page.html"), "<html><head></head><body></body></html>").expect("page html");

    let direct = XrefEntry {
        html: "crates/demo/src/mod.html".to_string(),
        imports: vec![link("dep/mod", "dep/mod.html")],
        imported_by: Vec::new(),
        symbols: vec!["Direct".to_string()],
        lsp_links: Vec::new(),
    };
    let adoc_entry = XrefEntry {
        html: "crates/demo/src/mod.html".to_string(),
        imports: Vec::new(),
        imported_by: vec![link("user/mod", "user/mod.html")],
        symbols: vec!["Merged".to_string()],
        lsp_links: Vec::new(),
    };

    let xref = HashMap::from([
        ("demo/mod".to_string(), direct),
        ("demo/page".to_string(), adoc_entry),
    ]);
    let existing = HashSet::from([
        "crates/demo/src/mod.html".to_string(),
        "docs/page.html".to_string(),
        "dep/mod.html".to_string(),
        "user/mod.html".to_string(),
    ]);
    let adoc_map = HashMap::from([("docs/page.html".to_string(), vec!["demo/page".to_string()])]);

    inject_xref(out, &xref, &existing, &adoc_map);

    let direct_html = fs::read_to_string(out.join("crates/demo/src/mod.html")).expect("direct read");
    assert!(direct_html.contains("window.__xref"));
    assert!(direct_html.contains("dep/mod.html"));

    let adoc_html = fs::read_to_string(out.join("docs/page.html")).expect("adoc read");
    assert!(adoc_html.contains("window.__xref"));
    assert!(adoc_html.contains("user/mod.html"));
    assert!(adoc_html.contains("Merged"));
}

// @
```


## Assembly


```rust
// <[@file weaveback-docgen/src/inject.rs]>=
// weaveback-docgen/src/inject.rs
// I'd Really Rather You Didn't edit this generated file.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use regex::Regex;
use std::sync::OnceLock;

use crate::xref::{html_path_for_key, XrefEntry, XrefLink};

// <[inject-rewrite]>
// <[inject-xref]>
// <[inject-merge]>
// <[inject-do]>
// <[inject-chunk-ids]>
#[cfg(test)]
mod tests;

// @
```

