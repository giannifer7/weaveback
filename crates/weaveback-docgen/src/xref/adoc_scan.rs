// weaveback-docgen/src/xref/adoc_scan.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use super::exclude::is_excluded;

pub(in crate::xref) fn atfile_re() -> &'static Regex {
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

