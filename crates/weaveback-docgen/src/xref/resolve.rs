// weaveback-docgen/src/xref/resolve.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub(in crate::xref) fn resolve_to_module(segments: &[&str], crate_dir: &Path, crate_name: &str) -> Option<String> {
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

pub(in crate::xref) fn resolve_import(
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

