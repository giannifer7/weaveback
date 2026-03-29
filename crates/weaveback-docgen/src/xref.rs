use std::collections::HashMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use weaveback_lsp::LspClient;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct XrefLink {
    pub key: String,
    pub label: String,
    /// HTML path relative to docs/html/
    pub html: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct XrefEntry {
    /// HTML path relative to docs/html/ for this module's own page (may not exist yet)
    pub html: String,
    pub imports: Vec<XrefLink>,
    pub imported_by: Vec<XrefLink>,
    pub symbols: Vec<String>,
    /// Precise semantic links from LSP
    #[serde(default)]
    pub lsp_links: Vec<XrefLink>,
}
/// `crates/weaveback-tangle/src/noweb.rs` → `weaveback_tangle/noweb`
pub fn module_key(rs_file: &Path, crates_dir: &Path) -> Option<String> {
    let rel = rs_file.strip_prefix(crates_dir).ok()?;
    let mut comps = rel.components();
    let crate_dir = comps.next()?.as_os_str().to_str()?;
    let crate_name = crate_dir.replace('-', "_");
    let src_seg = comps.next()?.as_os_str().to_str()?;
    if src_seg != "src" {
        return None;
    }
    let parts: Vec<&str> = comps
        .map(|c| c.as_os_str().to_str().unwrap_or(""))
        .collect();
    if parts.is_empty() {
        return None;
    }
    let mut path_parts = parts.clone();
    let last = path_parts.last_mut()?;
    *last = last.trim_end_matches(".rs");
    Some(format!("{}/{}", crate_name, path_parts.join("/")))
}

/// `weaveback_tangle/noweb` → `crates/weaveback-tangle/src/noweb.html`
pub fn html_path_for_key(key: &str) -> String {
    if let Some(slash_pos) = key.find('/') {
        let crate_name = &key[..slash_pos];
        let crate_dir = crate_name.replace('_', "-");
        let module_path = &key[slash_pos + 1..];
        format!("crates/{}/src/{}.html", crate_dir, module_path)
    } else {
        format!("{}.html", key)
    }
}
pub fn workspace_crate_names(crates_dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(entries) = std::fs::read_dir(crates_dir) else {
        return names;
    };
    for entry in entries.flatten() {
        let cargo = entry.path().join("Cargo.toml");
        if !cargo.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&cargo).unwrap_or_default();
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("name = ") {
                let name = rest.trim().trim_matches('"').replace('-', "_");
                names.push(name);
                break;
            }
        }
    }
    names
}
fn collect_use_tree(tree: &syn::UseTree, prefix: &str, out: &mut Vec<String>) {
    match tree {
        syn::UseTree::Path(p) => {
            let new_prefix = format!("{}{}::", prefix, p.ident);
            collect_use_tree(&p.tree, &new_prefix, out);
        }
        syn::UseTree::Name(n) => {
            out.push(format!("{}{}", prefix, n.ident));
        }
        syn::UseTree::Rename(r) => {
            out.push(format!("{}{}", prefix, r.ident));
        }
        syn::UseTree::Glob(_) => {
            // glob — record prefix so we know there's a dependency
            if !prefix.is_empty() {
                out.push(prefix.trim_end_matches("::").to_string());
            }
        }
        syn::UseTree::Group(g) => {
            for item in &g.items {
                collect_use_tree(item, prefix, out);
            }
        }
    }
}

fn is_pub(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

fn collect_items(items: &[syn::Item], use_paths: &mut Vec<String>, symbols: &mut Vec<String>) {
    for item in items {
        match item {
            syn::Item::Use(u) => {
                collect_use_tree(&u.tree, "", use_paths);
            }
            syn::Item::Fn(f) if is_pub(&f.vis) => {
                symbols.push(f.sig.ident.to_string());
            }
            syn::Item::Struct(s) if is_pub(&s.vis) => {
                symbols.push(s.ident.to_string());
            }
            syn::Item::Enum(e) if is_pub(&e.vis) => {
                symbols.push(e.ident.to_string());
            }
            syn::Item::Trait(t) if is_pub(&t.vis) => {
                symbols.push(t.ident.to_string());
            }
            syn::Item::Type(t) if is_pub(&t.vis) => {
                symbols.push(t.ident.to_string());
            }
            syn::Item::Const(c) if is_pub(&c.vis) => {
                symbols.push(c.ident.to_string());
            }
            syn::Item::Static(s) if is_pub(&s.vis) => {
                symbols.push(s.ident.to_string());
            }
            syn::Item::Mod(m) if is_pub(&m.vis) => {
                symbols.push(m.ident.to_string());
                if let Some((_, inner_items)) = &m.content {
                    collect_items(inner_items, use_paths, symbols);
                }
            }
            _ => {}
        }
    }
}

fn analyze_file(path: &Path) -> (Vec<String>, Vec<String>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (vec![], vec![]),
    };
    let file = match syn::parse_file(&content) {
        Ok(f) => f,
        Err(_) => return (vec![], vec![]),
    };
    let mut use_paths = Vec::new();
    let mut symbols = Vec::new();
    collect_items(&file.items, &mut use_paths, &mut symbols);
    (use_paths, symbols)
}
fn resolve_to_module(segments: &[&str], crate_dir: &Path, crate_name: &str) -> Option<String> {
    for len in (1..=segments.len()).rev() {
        let parts = &segments[..len];
        let rel: PathBuf = parts.iter().collect();
        let rs_file = crate_dir.join("src").join(&rel).with_extension("rs");
        let mod_file = crate_dir.join("src").join(&rel).join("mod.rs");
        if rs_file.exists() || mod_file.exists() {
            return Some(format!("{}/{}", crate_name, parts.join("/")));
        }
    }
    None
}

fn resolve_import(
    use_path: &str,
    current_key: &str,
    current_crate: &str,
    crates_dir: &Path,
    known_crates: &[String],
) -> Option<String> {
    if use_path.starts_with("super::") {
        let mut parts: Vec<&str> = current_key.split('/').collect();
        let mut remaining = use_path;
        while let Some(rest) = remaining.strip_prefix("super::") {
            remaining = rest;
            if parts.len() > 1 { parts.pop(); }
        }
        if parts.is_empty() { return None; }
        let crate_name = parts[0];
        let crate_dir = crates_dir.join(crate_name.replace('_', "-"));
        let prefix: Vec<&str> = parts[1..].to_vec();
        let segs: Vec<&str> = remaining.split("::").collect();
        let full: Vec<&str> = prefix.into_iter().chain(segs).collect();
        return resolve_to_module(&full, &crate_dir, crate_name);
    }

    if let Some(rest) = use_path.strip_prefix("crate::") {
        let segments: Vec<&str> = rest.split("::").collect();
        let crate_dir = crates_dir.join(current_crate.replace('_', "-"));
        return resolve_to_module(&segments, &crate_dir, current_crate);
    }
    for crate_name in known_crates {
        let prefix = format!("{}::", crate_name);
        if let Some(rest) = use_path.strip_prefix(prefix.as_str()) {
            let segments: Vec<&str> = rest.split("::").collect();
            let crate_dir = crates_dir.join(crate_name.replace('_', "-"));
            return resolve_to_module(&segments, &crate_dir, crate_name);
        }
    }
    None
}
fn atfile_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<<@file\s+([^>]+?)>>").unwrap())
}

pub fn scan_adoc_file_declarations(
    project_root: &Path,
    crates_dir: &Path,
) -> HashMap<String, Vec<String>> {
    let re = atfile_re();
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    let adoc_files: Vec<PathBuf> = walkdir::WalkDir::new(crates_dir)
        .into_iter()
        .filter_entry(|e| !is_excluded(e.path()))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "adoc"))
        .map(|e| e.into_path())
        .collect();

    for adoc in adoc_files {
        let content = match std::fs::read_to_string(&adoc) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let adoc_rel = match adoc.strip_prefix(project_root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        let adoc_html = format!("{}.html", adoc_rel.trim_end_matches(".adoc"));

        for cap in re.captures_iter(&content) {
            let file_path = cap[1].trim();
            // @file paths are relative to crates_dir
            let rs_path = crates_dir.join(file_path);
            if let Some(key) = module_key(&rs_path, crates_dir) {
                map.entry(adoc_html.clone()).or_default().push(key);
            }
        }
    }
    map
}
const EXCLUDE_DIRS: &[&str] = &["target", ".git", "gen", "node_modules", ".venv"];

fn is_excluded(path: &Path) -> bool {
    path.components().any(|c| {
        EXCLUDE_DIRS
            .iter()
            .any(|ex| c.as_os_str() == std::ffi::OsStr::new(ex))
    })
}
pub fn build_xref(project_root: &Path, use_lsp: bool) -> HashMap<String, XrefEntry> {
    let crates_dir = project_root.join("crates");
    let known_crates = workspace_crate_names(&crates_dir);

    let mut rs_files: Vec<PathBuf> = walkdir::WalkDir::new(&crates_dir)
        .into_iter()
        .filter_entry(|e| !is_excluded(e.path()))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
        .map(|e| e.into_path())
        .collect();
    rs_files.sort();

    struct RawData {
        path: PathBuf,
        use_paths: Vec<String>,
        symbols: Vec<String>,
    }
    let mut raw: HashMap<String, RawData> = HashMap::new();
    for file in &rs_files {
        let Some(key) = module_key(file, &crates_dir) else {
            continue;
        };
        let (use_paths, mut symbols) = analyze_file(file);
        symbols.sort();
        symbols.dedup();
        raw.insert(key, RawData { path: file.clone(), use_paths, symbols });
    }

    let mut fwd: HashMap<String, Vec<String>> = HashMap::new();
    let mut rev: HashMap<String, Vec<String>> = HashMap::new();

    for (key, data) in &raw {
        let current_crate = key.split('/').next().unwrap_or("");
        let mut resolved: Vec<String> = data
            .use_paths
            .iter()
            .filter_map(|p| resolve_import(p, key, current_crate, &crates_dir, &known_crates))
            .filter(|r| r != key)
            .collect();
        resolved.sort();
        resolved.dedup();
        for dep in &resolved {
            rev.entry(dep.clone()).or_default().push(key.clone());
        }
        fwd.insert(key.clone(), resolved);
    }
    for deps in rev.values_mut() {
        deps.sort();
        deps.dedup();
    }

    let make_link = |k: &str| XrefLink {
        key: k.to_string(),
        label: k.split('/').next_back().unwrap_or(k).to_string(),
        html: html_path_for_key(k),
    };

    let mut result: HashMap<String, XrefEntry> = HashMap::new();
    let mut keys: Vec<&String> = raw.keys().collect();
    keys.sort();
    for key in &keys {
        let data = &raw[*key];
        let imports = fwd
            .get(*key)
            .map(|v| v.iter().map(|k| make_link(k)).collect())
            .unwrap_or_default();
        let imported_by = rev
            .get(*key)
            .map(|v| v.iter().map(|k| make_link(k)).collect())
            .unwrap_or_default();
        result.insert(
            (*key).clone(),
            XrefEntry {
                html: html_path_for_key(key),
                imports,
                imported_by,
                symbols: data.symbols.clone(),
                lsp_links: vec![],
            },
        );
    }

    if use_lsp {
        let mut clients: HashMap<String, weaveback_lsp::LspClient> = HashMap::new();
        for key in keys {
            let data = &raw[key];
            let ext = data.path.extension().and_then(|e| e.to_str()).unwrap_or("");
            
            if let Some((lsp_cmd, lsp_lang)) = weaveback_lsp::get_lsp_config(ext) {
                if !clients.contains_key(&lsp_lang)
                    && let Ok(mut client) = weaveback_lsp::LspClient::spawn(&lsp_cmd, &[], project_root, lsp_lang.clone())
                    && client.initialize(project_root).is_ok()
                {
                    clients.insert(lsp_lang.clone(), client);
                }
                if let Some(client) = clients.get_mut(&lsp_lang) {
                    enrich_with_lsp(client, key, &data.path, &data.symbols, &mut result, &crates_dir);
                }
            }
        }
    }

    result
}

fn enrich_with_lsp(
    client: &mut LspClient,
    current_key: &str,
    path: &Path,
    symbols: &[String],
    result: &mut HashMap<String, XrefEntry>,
    crates_dir: &Path,
) {
    let _ = client.did_open(path);
    let content = std::fs::read_to_string(path).unwrap_or_default();
    for sym in symbols {
        // Simple heuristic: find symbol definition point
        let re = Regex::new(&format!(r"\b{}\b", sym)).unwrap();
        if let Some(m) = re.find(&content) {
            let (line, col) = find_line_col(&content, m.start());
            if let Ok(locs) = client.find_references(path, line - 1, col - 1) {
                for loc in locs {
                    if let Ok(target_path) = loc.uri.to_file_path()
                        && let Some(target_key) = module_key(&target_path, crates_dir)
                        && target_key != current_key
                        && let Some(entry) = result.get_mut(&current_key.to_string())
                    {
                        let label = format!("{} (ref)", target_key.split('/').next_back().unwrap_or(&target_key));
                        entry.lsp_links.push(XrefLink {
                            key: target_key.clone(),
                            label,
                            html: html_path_for_key(&target_key),
                        });
                    }
                }
            }
        }
    }
    // Dedup LSP links
    if let Some(entry) = result.get_mut(current_key) {
        entry.lsp_links.sort_by(|a, b| a.key.cmp(&b.key));
        entry.lsp_links.dedup_by(|a, b| a.key == b.key);
    }
}

fn find_line_col(text: &str, byte_offset: usize) -> (u32, u32) {
    let offset = byte_offset.min(text.len());
    let prefix = &text[..offset];
    let line_1 = prefix.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
    let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col_1 = prefix[line_start..].chars().count() as u32 + 1;
    (line_1, col_1)
}
