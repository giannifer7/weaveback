use std::collections::HashMap;
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
pub fn inject_xref(out_dir: &Path, xref: &HashMap<String, XrefEntry>) {
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
        do_inject(&html_file, entry);
    }
}

fn do_inject(html_file: &Path, entry: &XrefEntry) {
    let content = match std::fs::read_to_string(html_file) {
        Ok(c) => c,
        Err(_) => return,
    };
    if !content.contains("</head>") {
        return;
    }

    // Build a compact per-page xref object.
    // html paths are relative to docs/html/ — JavaScript can resolve from there.
    let xref_obj = serde_json::json!({
        "self":       entry.html,
        "imports":    entry.imports,
        "importedBy": entry.imported_by,
        "symbols":    entry.symbols,
    });

    let tag = format!(
        "<script>window.__xref={}</script>\n",
        xref_obj
    );
    let patched = content.replacen("</head>", &format!("{}</head>", tag), 1);
    let _ = std::fs::write(html_file, patched);
}
