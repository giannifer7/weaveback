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

/// Generate `literate-index.html` listing all weaveback-macro literate source
/// pages, then inject a link to it into `README.html`.
pub fn generate_and_inject(out_dir: &Path) {
    let src_dir = out_dir
        .join("crates")
        .join("weaveback-macro")
        .join("src");
    if !src_dir.exists() {
        return;
    }

    // href (relative to out_dir) → title, grouped by first path component under src_dir.
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
        let rel_out = abs.strip_prefix(out_dir).unwrap();
        let href = rel_out.to_string_lossy().replace('\\', "/");

        // Group by first component; top-level files (e.g. line_index.html) get
        // their own group labelled "(top-level)".
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

    let mut body = String::new();
    for (group, entries) in &groups {
        let heading = if group == "(top-level)" {
            "Top-level modules"
        } else {
            group.as_str()
        };
        body.push_str(&format!("<h2>{heading}</h2>\n<ul>\n"));
        for (href, title) in entries {
            body.push_str(&format!("<li><a href=\"{href}\">{title}</a></li>\n"));
        }
        body.push_str("</ul>\n");
    }

    let total: usize = groups.values().map(|v| v.len()).sum();
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>weaveback-macro — Literate Source Index</title>
<style>
body  {{ background:#1d2021; color:#ebdbb2; font-family:sans-serif;
         max-width:860px; margin:2em auto; padding:0 1.5em; line-height:1.6; }}
h1   {{ color:#b8bb26; }}
h2   {{ color:#83a598; border-bottom:1px solid #3c3836; padding-bottom:.2em;
        margin-top:1.8em; text-transform:capitalize; }}
a    {{ color:#83a598; text-decoration:none; }}
a:hover {{ color:#8ec07c; text-decoration:underline; }}
ul   {{ list-style:none; padding-left:0; }}
li   {{ margin:.35em 0; }}
.back {{ margin-bottom:1.5em; font-size:.95em; }}
</style>
</head>
<body>
<p class="back"><a href="index.html">← README</a></p>
<h1>weaveback-macro — Literate Source Index</h1>
{body}
</body>
</html>
"#
    );

    let index_path = out_dir.join("literate-index.html");
    if let Err(e) = std::fs::write(&index_path, &html) {
        eprintln!("docs: failed to write literate-index.html: {e}");
    } else {
        println!("docs: wrote literate-index.html ({total} pages)");
    }

    inject_link_into_readme(out_dir);
}

fn inject_link_into_readme(out_dir: &Path) {
    let readme_path = out_dir.join("README.html");
    let Ok(content) = std::fs::read_to_string(&readme_path) else {
        return;
    };
    if content.contains("literate-index.html") {
        return;
    }

    let link_html = concat!(
        "<div style=\"margin:1.5em 0;padding:1em 1.2em;",
        "background:#282828;border-left:4px solid #83a598;\">",
        "<a href=\"literate-index.html\" ",
        "style=\"color:#83a598;font-size:1.05em;font-weight:bold;\">",
        "weaveback-macro implementation index</a>",
        "<span style=\"color:#a89984;\"> — literate source documentation for all modules</span>",
        "</div>",
    );

    // Inject immediately after <div id="content"> so it appears at the top of
    // the page body, above the first section.
    let injection_point = "<div id=\"content\">";
    let patched = if let Some(pos) = content.find(injection_point) {
        let insert_at = pos + injection_point.len();
        format!("{}\n{}{}", &content[..insert_at], link_html, &content[insert_at..])
    } else {
        content.replacen("</body>", &format!("{}\n</body>", link_html), 1)
    };

    if let Err(e) = std::fs::write(&readme_path, &patched) {
        eprintln!("docs: failed to inject link into README.html: {e}");
    }
}
