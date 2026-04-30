// weaveback-api/src/coverage/lcov/summary/generated_lookup.rs
// I'd Really Rather You Didn't edit this generated file.

use super::super::*;

pub(in crate::coverage) fn find_noweb_entries_for_generated_file(
    db: &weaveback_tangle::db::WeavebackDb,
    file_name: &str,
    project_root: &Path,
) -> Option<Vec<(u32, weaveback_tangle::db::NowebMapEntry)>> {
    let mut candidates = Vec::new();
    candidates.push(file_name.to_string());
    let file_path = Path::new(file_name);
    if let Ok(rel) = file_path.strip_prefix(project_root) {
        let rel = rel.to_string_lossy().replace('\\', "/");
        if !candidates.contains(&rel) {
            candidates.push(rel);
        }
    }

    for candidate in candidates {
        if let Ok(entries) = db.get_noweb_entries_for_file(&candidate)
            && !entries.is_empty()
        {
            return Some(entries);
        }
        for suffix in distinctive_suffix_candidates(&candidate) {
            if let Ok(entries) = db.get_noweb_entries_for_file_by_suffix(&suffix)
                && !entries.is_empty()
            {
                return Some(entries);
            }
        }
    }

    None
}

