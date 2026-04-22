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

