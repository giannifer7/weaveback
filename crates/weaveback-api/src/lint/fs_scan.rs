// weaveback-api/src/lint/fs_scan.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub(in crate::lint) fn should_skip_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    matches!(
        name,
        ".git"
            | "target"
            | "node_modules"
            | ".venv"
            | ".plantuml-cache"
            | "__pycache__"
            | "expanded-adoc"
            | "expanded-md"
    ) || path.ends_with("docs/html")
}

pub(in crate::lint) fn is_lint_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| matches!(ext, "adoc" | "wvb"))
}

pub(in crate::lint) fn is_wvb_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("wvb")
}

pub(in crate::lint) fn is_prelude_file(path: &Path) -> bool {
    path.components().any(|component| component.as_os_str() == "prelude")
}

pub(in crate::lint) fn collect_literate_files(path: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_file() {
        if is_lint_source_file(path) {
            out.push(path.to_path_buf());
        }
        return Ok(());
    }
    if !path.is_dir() || should_skip_dir(path) {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() {
            if should_skip_dir(&child) {
                continue;
            }
            collect_literate_files(&child, out)?;
        } else if is_lint_source_file(&child) {
            out.push(child);
        }
    }
    Ok(())
}

