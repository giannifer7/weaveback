// weaveback-docgen/src/xref/module_key.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

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

