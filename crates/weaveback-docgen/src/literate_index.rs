use std::collections::BTreeMap;
use std::path::Path;

fn extract_title(html: &str) -> String {
    if let Some(start) = html.find("<title>") {
        let rest = &html[start + 7..];
        if let Some(end) = rest.find("</title>") {
            return rest[..end].trim().to_string();
        }
    }
    String::new()
}

/// Inject an "Implementation pages" section into `weaveback_macro.html`,
/// listing every literate source page under `weaveback-macro/src/`.
pub fn generate_and_inject(out_dir: &Path) {
    let src_dir = out_dir
        .join("crates")
        .join("weaveback-macro")
        .join("src");
    if !src_dir.exists() {
        return;
    }

    // href relative to src_dir → title, grouped by first path component.
    let mut groups: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();

    for entry in walkdir::WalkDir::new(&src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            !e.file_type().is_dir()
                && e.path().extension().is_some_and(|x| x == "html")
        })
    {
        let abs = entry.into_path();
        let rel_src = abs.strip_prefix(&src_dir).unwrap();
        let rel_str = rel_src.to_string_lossy().replace('\\', "/");

        // Skip the page we're injecting into
        if rel_str == "weaveback_macro.html" {
            continue;
        }

        // href is relative to src_dir (same directory as weaveback_macro.html)
        let href = rel_str.clone();

        let first = rel_src
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_default();
        let group = if first.ends_with(".html") {
            "(top-level)".to_string()
        } else {
            first
        };

        let title = std::fs::read_to_string(&abs)
            .ok()
            .map(|s| extract_title(&s))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| {
                abs.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });

        groups.entry(group).or_default().push((href, title));
    }

    for entries in groups.values_mut() {
        entries.sort_by(|a, b| a.1.cmp(&b.1));
    }

    if groups.is_empty() {
        return;
    }

    let total: usize = groups.values().map(|v| v.len()).sum();

    let mut inner = String::new();
    for (group, entries) in &groups {
        let heading = if group == "(top-level)" {
            "Top-level modules".to_string()
        } else {
            let mut s = group.clone();
            s[..1].make_ascii_uppercase();
            s
        };
        inner.push_str(&format!(
            "<div class=\"sect2\">\n<h3>{heading}</h3>\n<div class=\"ulist\">\n<ul>\n"
        ));
        for (href, title) in entries {
            inner.push_str(&format!(
                "<li><p><a href=\"{href}\">{title}</a></p></li>\n"
            ));
        }
        inner.push_str("</ul>\n</div>\n</div>\n");
    }

    let section = format!(
        concat!(
            "<div class=\"sect1\" id=\"literate-sources\">\n",
            "<h2>Implementation pages</h2>\n",
            "<div class=\"sectionbody\">\n",
            "<p>weaveback-macro is written as a literate program. ",
            "Each module has a page combining prose, diagrams, and the full source:</p>\n",
            "{inner}",
            "</div>\n</div>\n"
        ),
        inner = inner
    );

    inject_into_page(&src_dir.join("weaveback_macro.html"), &section, total);
}

fn inject_into_page(page: &Path, section: &str, total: usize) {
    let Ok(content) = std::fs::read_to_string(page) else {
        eprintln!("docs: could not read {}", page.display());
        return;
    };

    // Strip previous injection to keep the operation idempotent
    let content = strip_existing(&content);

    let marker = "<div id=\"footer\">";
    let patched = if let Some(pos) = content.find(marker) {
        format!("{}{}{}", &content[..pos], section, &content[pos..])
    } else {
        content.replacen("</body>", &format!("{}\n</body>", section), 1)
    };

    if let Err(e) = std::fs::write(page, &patched) {
        eprintln!("docs: failed to inject literate index into {}: {e}", page.display());
    } else {
        println!("docs: injected literate index into weaveback_macro.html ({total} pages)");
    }
}

fn strip_existing(content: &str) -> String {
    const START: &str = "<div class=\"sect1\" id=\"literate-sources\">";
    const END: &str = "</div>\n</div>\n";
    let mut s = content.to_string();
    if let Some(start) = s.find(START) {
        // Find the matching double-close that ends the sect1 > sectionbody
        let after = &s[start + START.len()..];
        if let Some(rel_end) = after.find(END) {
            let end = start + START.len() + rel_end + END.len();
            s = format!("{}{}", &s[..start], &s[end..]);
        }
    }
    s
}
