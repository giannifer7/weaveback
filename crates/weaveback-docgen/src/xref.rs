use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── Public types ──────────────────────────────────────────────────────────────

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
}

// ── Module key helpers ────────────────────────────────────────────────────────

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

// ── Workspace crate discovery ─────────────────────────────────────────────────

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

// ── File analysis ─────────────────────────────────────────────────────────────

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

// ── Import resolution ─────────────────────────────────────────────────────────

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
    current_crate: &str,
    crates_dir: &Path,
    known_crates: &[String],
) -> Option<String> {
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

// ── EXCLUDE dirs ──────────────────────────────────────────────────────────────

const EXCLUDE_DIRS: &[&str] = &["target", ".git", "gen", "node_modules", ".venv"];

fn is_excluded(path: &Path) -> bool {
    path.components().any(|c| {
        EXCLUDE_DIRS
            .iter()
            .any(|ex| c.as_os_str() == std::ffi::OsStr::new(ex))
    })
}

// ── Main entry point ──────────────────────────────────────────────────────────

pub fn build_xref(project_root: &Path) -> HashMap<String, XrefEntry> {
    let crates_dir = project_root.join("crates");
    let known_crates = workspace_crate_names(&crates_dir);

    // Collect .rs files
    let mut rs_files: Vec<PathBuf> = walkdir::WalkDir::new(&crates_dir)
        .into_iter()
        .filter_entry(|e| !is_excluded(e.path()))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
        .map(|e| e.into_path())
        .collect();
    rs_files.sort();

    // Analyze each file
    struct RawData {
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
        raw.insert(key, RawData { use_paths, symbols });
    }

    // Resolve imports, build forward + reverse index
    let mut fwd: HashMap<String, Vec<String>> = HashMap::new();
    let mut rev: HashMap<String, Vec<String>> = HashMap::new();

    for (key, data) in &raw {
        let current_crate = key.split('/').next().unwrap_or("");
        let mut resolved: Vec<String> = data
            .use_paths
            .iter()
            .filter_map(|p| resolve_import(p, current_crate, &crates_dir, &known_crates))
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

    // Build XrefEntry map
    let make_link = |k: &str| XrefLink {
        key: k.to_string(),
        label: k.split('/').next_back().unwrap_or(k).to_string(),
        html: html_path_for_key(k),
    };

    let mut result: HashMap<String, XrefEntry> = HashMap::new();
    let mut keys: Vec<&String> = raw.keys().collect();
    keys.sort();
    for key in keys {
        let data = &raw[key];
        let imports = fwd
            .get(key)
            .map(|v| v.iter().map(|k| make_link(k)).collect())
            .unwrap_or_default();
        let imported_by = rev
            .get(key)
            .map(|v| v.iter().map(|k| make_link(k)).collect())
            .unwrap_or_default();
        result.insert(
            key.clone(),
            XrefEntry {
                html: html_path_for_key(key),
                imports,
                imported_by,
                symbols: data.symbols.clone(),
            },
        );
    }
    result
}
