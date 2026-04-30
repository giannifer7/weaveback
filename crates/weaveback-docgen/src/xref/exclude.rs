// weaveback-docgen/src/xref/exclude.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

const EXCLUDE_DIRS: &[&str] = &["target", ".git", "gen", "node_modules", ".venv"];

pub(in crate::xref) fn is_excluded(path: &Path) -> bool {
    path.components().any(|c| {
        EXCLUDE_DIRS
            .iter()
            .any(|ex| c.as_os_str() == std::ffi::OsStr::new(ex))
    })
}

