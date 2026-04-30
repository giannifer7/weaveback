// weaveback-docgen/src/xref/build.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use super::analysis::analyze_file;
use super::exclude::is_excluded;
use super::resolve::resolve_import;
use super::workspace::workspace_crate_names;

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

pub(in crate::xref) fn enrich_with_lsp(
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

pub(in crate::xref) fn find_line_col(text: &str, byte_offset: usize) -> (u32, u32) {
    let offset = byte_offset.min(text.len());
    let prefix = &text[..offset];
    let line_1 = prefix.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
    let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col_1 = prefix[line_start..].chars().count() as u32 + 1;
    (line_1, col_1)
}

