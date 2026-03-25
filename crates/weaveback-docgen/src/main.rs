mod inject;
mod literate_index;
mod render;
mod xref;

use std::path::PathBuf;

fn find_project_root() -> PathBuf {
    let mut dir = std::env::current_dir().expect("cannot determine cwd");
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = std::fs::read_to_string(&cargo_toml).unwrap_or_default();
            if content.contains("[workspace]") {
                return dir;
            }
        }
        if !dir.pop() {
            break;
        }
    }
    std::env::current_dir().unwrap()
}

fn main() {
    let root = find_project_root();
    let out_dir = root.join("docs").join("html");
    let theme_dir = root.join("scripts").join("asciidoc-theme");

    // 1. Render .adoc → HTML
    let all_html = render::render_docs(&root, &theme_dir, &out_dir);
    let existing_html: std::collections::HashSet<String> = all_html
        .iter()
        .filter_map(|p| p.strip_prefix(&out_dir).ok())
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .collect();

    // 2. Build Rust xref graph from crates/**/*.rs
    println!("xref: analysing crates...");
    let crates_dir = root.join("crates");
    let xref = xref::build_xref(&root);
    let adoc_map = xref::scan_adoc_file_declarations(&root, &crates_dir);
    println!("xref: {} modules indexed, {} adoc overrides", xref.len(), adoc_map.len());

    // 3. Write xref.json alongside the HTML output
    let xref_json_path = out_dir.join("xref.json");
    match serde_json::to_string_pretty(&xref) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&xref_json_path, &json) {
                eprintln!("xref: could not write {}: {}", xref_json_path.display(), e);
            } else {
                println!("xref: wrote {}", xref_json_path.display());
            }
        }
        Err(e) => eprintln!("xref: serialisation error: {}", e),
    }

    // 4. Rewrite .adoc hrefs to .html in all generated pages
    inject::rewrite_adoc_links(&out_dir);

    // 5. Inject per-page window.__xref into HTML files that have a matching entry
    inject::inject_xref(&out_dir, &xref, &existing_html, &adoc_map);

    // 6. Generate literate source index and link it from README.html
    literate_index::generate_and_inject(&out_dir);
}
