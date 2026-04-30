// weaveback-docgen/src/xref/workspace.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub(crate) fn workspace_crate_names(crates_dir: &Path) -> Vec<String> {
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

