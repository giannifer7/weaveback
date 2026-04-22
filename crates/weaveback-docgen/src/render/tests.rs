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

