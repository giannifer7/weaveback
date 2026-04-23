# Rust cross-reference graph

`xref.rs` builds a forward-and-reverse cross-reference graph from the
workspace Rust source files.  It uses `syn` to parse each `.rs` file, collects
`use` declarations and public symbol definitions, resolves import paths to
module keys, and produces a `HashMap<String, XrefEntry>` mapping each module
key to its import edges and public symbols.

The graph is consumed by [`inject_xref`](../src-wvb/inject.wvb) to embed
`window.__xref` data in generated HTML pages, and serialised to `xref.json`
for the JavaScript side panel.

See [weaveback_docgen.wvb](weaveback_docgen.wvb) for the module map.

## Module key scheme

A module key is a `/`-separated string derived from a `.rs` path relative to
`crates/`, with hyphens in the crate directory converted to underscores:

....
crates/weaveback-tangle/src/noweb.rs          →  weaveback_tangle/noweb
crates/weaveback-macro/src/evaluator/core.rs  →  weaveback_macro/evaluator/core
....

`html_path_for_key` is the inverse: it reconstructs the relative HTML path,
converting underscores back to hyphens in the crate segment.

## Public types

`XrefLink` is a single directed edge.  `XrefEntry` collects all edges for one
module together with its public symbol list.  Both types derive `Serialize` and
`Deserialize` so they can be written to `xref.json` and embedded as JSON in
HTML pages.

```rust
// <[xref-types]>=
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
// @
```


## Module key helpers

```rust
// <[xref-module-key]>=
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
// @
```


## Workspace crate discovery

`workspace_crate_names` scans `crates/` for `Cargo.toml` files and extracts
crate names (normalised to underscore form).  The list is used by
`resolve_import` to recognise cross-crate `use` paths.

```rust
// <[xref-workspace]>=
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
// @
```


## Syn-based file analysis

`collect_use_tree` recursively walks a `syn::UseTree` and records all
fully-qualified import paths.  Globs are recorded as the prefix path so that
`use foo::bar::*` still creates a dependency edge to `foo::bar`.

`collect_items` iterates the top-level items of a parsed file: `use`
statements feed into `collect_use_tree`; publicly-visible types, functions,
constants, and modules feed into the `symbols` list.

`analyze_file` reads and parses a `.rs` file, returning `(use_paths,
symbols)`.  Parse failures are silently ignored — an empty result is safe
because the module will simply have no edges in the graph.

```rust
// <[xref-use-tree]>=
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
// @
```


## Import resolution

`resolve_to_module` probes for `{segments}.rs` and `{segments}/mod.rs` in the
crate's `src/` directory, trying progressively shorter prefixes so that `use
foo::Bar` resolves to module `foo` even though `Bar` is a type, not a module.

`resolve_import` dispatches on the import prefix:

* `super::` — pop levels from the current key path, then resolve the remainder
* `crate::` — resolve relative to the current crate root
* `{known_crate}::` — resolve in the named workspace crate

Anything else (standard library, external crates, trait-only imports) is
silently dropped and produces no graph edge.

```rust
// <[xref-resolve]>=
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
// @
```


## Adoc @file declaration scanning

Some `.adoc` files generate `.rs` files whose name differs from the `.adoc`
stem (e.g. `cli.adoc` generates `weaveback-macro.rs`).  The xref injection
must match these pages to the right graph entries.

`scan_adoc_file_declarations` walks `.adoc` files under `crates_dir`,
extracts `<<@file path>>=` declarations with a regex, and maps each adoc's
HTML path to the list of module keys those declarations produce.

NOTE: This regex only recognises the `<< >>` delimiter style used by
weaveback-macro adocs.  Crates that use other delimiters (e.g. `<[ ]>`) will
not be scanned here; their module pages are handled by the direct key lookup
in `inject_xref` so long as the `.adoc` stem matches the `.rs` stem.

```rust
// <[xref-adoc-scan]>=
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
// @
```


## Excluded paths

`EXCLUDE_DIRS` lists directories skipped by all walkers.  `gen` is excluded
here but not in `render.rs` because xref analysis only needs original source
files, not generated output.

```rust
// <[xref-exclude]>=
const EXCLUDE_DIRS: &[&str] = &["target", ".git", "gen", "node_modules", ".venv"];

fn is_excluded(path: &Path) -> bool {
    path.components().any(|c| {
        EXCLUDE_DIRS
            .iter()
            .any(|ex| c.as_os_str() == std::ffi::OsStr::new(ex))
    })
}
// @
```


## build_xref

`build_xref` ties all the pieces together: discover `.rs` files, analyse each
with `syn`, resolve imports into module keys, and build the forward and reverse
edges.  The local `RawData` struct accumulates per-file data before resolution
so that the two passes (forward edge collection, reverse edge derivation) can
share the same analysis results.

```rust
// <[xref-build]>=
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
// @
```


## Assembly

```rust
// <[@file weaveback-docgen/src/xref.rs]>=
// weaveback-docgen/src/xref.rs
// I'd Really Rather You Didn't edit this generated file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use weaveback_lsp::LspClient;

// <[xref-types]>
// <[xref-module-key]>
// <[xref-workspace]>
// <[xref-use-tree]>
// <[xref-resolve]>
// <[xref-adoc-scan]>
// <[xref-exclude]>
// <[xref-build]>
#[cfg(test)]
mod tests;

// @
```


## Tests

The test body is generated as `xref/tests.rs` and linked from
`xref.rs` with `#[cfg(test)] mod tests;`.

```rust
// <[@file weaveback-docgen/src/xref/tests.rs]>=
// weaveback-docgen/src/xref/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn module_and_html_keys_roundtrip_basic_workspace_paths() {
    let crates_dir = Path::new("/tmp/ws/crates");
    let rs = crates_dir.join("weaveback-tangle/src/noweb.rs");
    assert_eq!(module_key(&rs, crates_dir).as_deref(), Some("weaveback_tangle/noweb"));
    assert_eq!(html_path_for_key("weaveback_tangle/noweb"), "crates/weaveback-tangle/src/noweb.html");
    assert_eq!(html_path_for_key("index"), "index.html");
}

#[test]
fn collect_and_resolve_imports_cover_common_rust_forms() {
    let dir = tempdir().expect("tempdir");
    let crates_dir = dir.path().join("crates");
    fs::create_dir_all(crates_dir.join("demo/src/parser")).expect("parser dir");
    fs::write(
        crates_dir.join("demo/Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .expect("cargo");
    fs::write(crates_dir.join("demo/src/lib.rs"), "pub mod parser;\n").expect("lib");
    fs::write(crates_dir.join("demo/src/parser/mod.rs"), "pub fn parse() {}\n").expect("parser");

    let known = workspace_crate_names(&crates_dir);
    assert_eq!(known, vec!["demo".to_string()]);

    let demo_file = crates_dir.join("demo/src/demo.rs");
    fs::write(
        &demo_file,
        "pub use crate::parser::parse;\nuse crate::{parser::parse as parse2};\npub struct Demo;\n",
    )
    .expect("demo");

    let (uses, symbols) = analyze_file(&demo_file);
    assert!(uses.iter().any(|u| u == "crate::parser::parse"));
    assert!(symbols.iter().any(|s| s == "Demo"));

    assert_eq!(
        resolve_to_module(&["parser", "parse"], &crates_dir.join("demo"), "demo").as_deref(),
        Some("demo/parser")
    );
    assert_eq!(
        resolve_import("crate::parser::parse", "demo/demo", "demo", &crates_dir, &known).as_deref(),
        Some("demo/parser")
    );
    assert_eq!(
        resolve_import("super::parser::parse", "demo/nested/mod", "demo", &crates_dir, &known).as_deref(),
        None
    );
}

#[test]
fn collect_use_tree_and_collect_items_cover_groups_globs_and_pub_items() {
    let tree: syn::UseTree = syn::parse_str("crate::{alpha::Beta, gamma::*, delta as renamed}").expect("use tree");
    let mut out = Vec::new();
    collect_use_tree(&tree, "", &mut out);
    assert_eq!(
        out,
        vec![
            "crate::alpha::Beta".to_string(),
            "crate::gamma".to_string(),
            "crate::delta".to_string(),
        ]
    );

    let file = syn::parse_file(
        "pub fn hello() {}\nstruct Hidden;\npub mod inner { pub struct Visible; }\nuse crate::alpha::Beta;\n",
    )
    .expect("file");
    let mut use_paths = Vec::new();
    let mut symbols = Vec::new();
    collect_items(&file.items, &mut use_paths, &mut symbols);
    assert!(use_paths.iter().any(|u| u == "crate::alpha::Beta"));
    assert!(symbols.iter().any(|s| s == "hello"));
    assert!(symbols.iter().any(|s| s == "inner"));
    assert!(symbols.iter().any(|s| s == "Visible"));
    assert!(!symbols.iter().any(|s| s == "Hidden"));
}

#[test]
fn adoc_scan_and_line_col_helpers_work() {
    let dir = tempdir().expect("tempdir");
    let project_root = dir.path();
    let crates_dir = project_root.join("crates");
    fs::create_dir_all(crates_dir.join("demo/src")).expect("src dir");
    fs::write(
        crates_dir.join("demo/Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .expect("cargo");
    fs::write(crates_dir.join("demo/src/lib.rs"), "pub fn hello() {}\n").expect("lib");
    fs::write(
        crates_dir.join("demo/src/lib.adoc"),
        "// <<@file demo/src/lib.rs>>=\nbody\n// @\n",
    )
    .expect("adoc");

    let map = scan_adoc_file_declarations(project_root, &crates_dir);
    assert_eq!(
        map.get("crates/demo/src/lib.html"),
        Some(&vec!["demo/lib".to_string()])
    );
    assert_eq!(find_line_col("ab\ncde", 0), (1, 1));
    assert_eq!(find_line_col("ab\ncde", 3), (2, 1));
    assert_eq!(find_line_col("ab\ncde", 5), (2, 3));
}

#[test]
fn is_excluded_matches_expected_workspace_noise_dirs() {
    assert!(is_excluded(Path::new("/tmp/project/target/file.rs")));
    assert!(is_excluded(Path::new("/tmp/project/.git/config")));
    assert!(is_excluded(Path::new("/tmp/project/node_modules/pkg/index.js")));
    assert!(!is_excluded(Path::new("/tmp/project/crates/demo/src/lib.rs")));
}

#[test]
fn test_build_xref_orchestration() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    let crates_dir = root.join("crates");

    let crate_a = crates_dir.join("crate-a");
    fs::create_dir_all(crate_a.join("src")).unwrap();
    fs::write(crate_a.join("Cargo.toml"), "[package]\nname = \"crate-a\"\n").unwrap();
    fs::write(crate_a.join("src/lib.rs"), "pub mod sub;").unwrap();
    fs::write(crate_a.join("src/sub.rs"), "pub struct Alpha;").unwrap();

    let crate_b = crates_dir.join("crate-b");
    fs::create_dir_all(crate_b.join("src")).unwrap();
    fs::write(crate_b.join("Cargo.toml"), "[package]\nname = \"crate-b\"\n").unwrap();
    fs::write(crate_b.join("src/lib.rs"), "use crate_a::sub::Alpha; pub struct Beta;").unwrap();

    let xref = build_xref(root, false);
    assert!(xref.contains_key("crate_a/sub"));
    assert!(xref.contains_key("crate_b/lib"));

    let a = xref.get("crate_a/sub").unwrap();
    assert!(a.symbols.contains(&"Alpha".to_string()));
    assert!(a.imported_by.iter().any(|l| l.key == "crate_b/lib"));

    let b = xref.get("crate_b/lib").unwrap();
    assert!(b.imports.iter().any(|l| l.key == "crate_a/sub"));
}

// @
```

