use std::collections::{HashMap, HashSet};
use std::path::Path;

use regex::Regex;
use std::sync::OnceLock;

use crate::xref::{html_path_for_key, XrefEntry};

// ── .adoc → .html link rewriting ─────────────────────────────────────────────

fn adoc_href_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Match href="…something.adoc…" — capture the .adoc extension only
    RE.get_or_init(|| Regex::new(r#"(href="[^"]*?)\.adoc([^"]*?")"#).unwrap())
}

/// Rewrite every `href="….adoc…"` to `href="….html…"` in all HTML files
/// under `out_dir`. Runs after asciidoctor so cross-doc links resolve correctly.
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

/// Inject `<script>window.__xref=…</script>` before `</head>` in each HTML
/// page that has a corresponding xref entry.
/// `existing_html` is the set of HTML paths (relative to `out_dir`, `/`-separated)
/// that were actually rendered; links to absent pages are filtered out.
pub fn inject_xref(
    out_dir: &Path,
    xref: &HashMap<String, XrefEntry>,
    existing_html: &HashSet<String>,
) {
    // Build a reverse map: html relative path (normalized with /) → key
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

    for html_file in html_files {
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
        do_inject(&html_file, entry, existing_html);
    }
}

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
