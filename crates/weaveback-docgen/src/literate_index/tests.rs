// weaveback-docgen/src/literate_index/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn extract_title_and_strip_existing_handle_missing_markers() {
    assert_eq!(extract_title("<html><title>Hello</title></html>"), "Hello");
    assert_eq!(extract_title("<html></html>"), "");

    let original = concat!(
        "<body>",
        "<div class=\"sect1\" id=\"literate-sources\">",
        "<h2>Implementation pages</h2>",
        "</div>\n</div>\n",
        "<div id=\"footer\"></div></body>"
    );
    let stripped = strip_existing(original);
    assert!(!stripped.contains("literate-sources"));
    assert!(stripped.contains("<div id=\"footer\">"));
}

#[test]
fn inject_into_page_inserts_section_before_footer_and_is_idempotent() {
    let dir = tempdir().expect("tempdir");
    let page = dir.path().join("index.html");
    fs::write(&page, "<html><body><p>Intro</p><div id=\"footer\"></div></body></html>").expect("page");

    inject_into_page(&page, "<div class=\"sect1\" id=\"literate-sources\">X</div>\n</div>\n", 1, "demo");
    inject_into_page(&page, "<div class=\"sect1\" id=\"literate-sources\">X</div>\n</div>\n", 1, "demo");

    let content = fs::read_to_string(&page).expect("read page");
    assert_eq!(content.matches("literate-sources").count(), 1);
    assert!(content.find("literate-sources").expect("section") < content.find("footer").expect("footer"));
}

#[test]
fn generate_and_inject_crate_builds_grouped_module_section() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src");
    fs::create_dir_all(src.join("parser")).expect("parser dir");
    let index = src.join("demo.html");
    fs::write(&index, "<html><body><div id=\"footer\"></div></body></html>").expect("index");
    fs::write(src.join("top.html"), "<html><title>Top Title</title></html>").expect("top");
    fs::write(src.join("parser").join("mod.html"), "<html><title>Parser Mod</title></html>").expect("mod");

    generate_and_inject_crate(&src, &index, "demo");

    let content = fs::read_to_string(&index).expect("read index");
    assert!(content.contains("Implementation pages"));
    assert!(content.contains("Top-level modules"));
    assert!(content.contains("Parser"));
    assert!(content.contains("top.html"));
    assert!(content.contains("parser/mod.html"));
}

