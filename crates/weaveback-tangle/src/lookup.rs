use std::path::Path;
use crate::db::{WeavebackDb, NowebMapEntry, DbError};
use weaveback_core::PathResolver;

/// Returns (line, col) both 1-indexed; col counts UTF-8 characters.
pub fn find_line_col(text: &str, byte_offset: usize) -> (u32, u32) {
    let offset = byte_offset.min(text.len());
    let prefix = &text[..offset];
    let line_1 = prefix.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
    let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col_1 = prefix[line_start..].chars().count() as u32 + 1;
    (line_1, col_1)
}

/// Attempt to find a noweb-map entry for `out_file`.  Tries the raw path,
/// then the normalized path.
pub fn find_best_noweb_entry(
    db: &WeavebackDb,
    out_file: &str,
    out_line_0: u32,
    resolver: &PathResolver,
) -> Result<Option<NowebMapEntry>, DbError> {
    // Try 1: Exact match as provided.
    if let Some(entry) = db.get_noweb_entry(out_file, out_line_0)? {
        return Ok(Some(entry));
    }

    // Try 2: Normalized via PathResolver.
    let norm = resolver.normalize(out_file);
    if norm != out_file && let Some(entry) = db.get_noweb_entry(&norm, out_line_0)? {
        return Ok(Some(entry));
    }

    Ok(None)
}

/// Attempt to find a source configuration for `src_file`. Tries the raw path,
/// then strips/adds common prefixes.
pub fn find_best_source_config(
    db: &WeavebackDb,
    src_file: &str,
) -> Result<Option<crate::db::TangleConfig>, DbError> {
    // Try 1: Exact match as provided.
    if let Some(cfg) = db.get_source_config(src_file)? {
        return Ok(Some(cfg));
    }

    // Try 2: If it starts with ./, try without it.
    if let Some(stripped) = src_file.strip_prefix("./") 
        && let Some(cfg) = db.get_source_config(stripped)? {
        return Ok(Some(cfg));
    }

    // Try 3: Try with ./ if it doesn't have it.
    if !src_file.starts_with("./") && !src_file.starts_with('/') {
        let dotted = format!("./{}", src_file);
        if let Some(cfg) = db.get_source_config(&dotted)? {
            return Ok(Some(cfg));
        }
    }

    // Try 4: Strip common workspace prefixes.
    for prefix in &["crates/"] {
        if let Some(stripped) = src_file.strip_prefix(prefix) {
            if let Some(cfg) = db.get_source_config(stripped)? {
                return Ok(Some(cfg));
            }
            let dotted = format!("./{}", stripped);
            if let Some(cfg) = db.get_source_config(&dotted)? {
                return Ok(Some(cfg));
            }
        }
    }

    // Try 5: If absolute, try matching relative to cwd or just the file name.
    let path = Path::new(src_file);
    if path.is_absolute() {
        if let Ok(cwd) = std::env::current_dir()
            && let Ok(rel) = path.strip_prefix(&cwd) {
            let rel_str = rel.to_string_lossy();
            if let Some(cfg) = find_best_source_config(db, &rel_str)? {
                return Ok(Some(cfg));
            }
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(cfg) = db.get_source_config(name)? {
                return Ok(Some(cfg));
            }
            let dotted = format!("./{}", name);
            if let Some(cfg) = db.get_source_config(&dotted)? {
                return Ok(Some(cfg));
            }
        }
    }

    Ok(None)
}
