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
/// then the normalized path, then a canonical absolute path when the file
/// exists on disk.
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

    // Try 3: Canonical on-disk path. This helps external tools like cargo and
    // llvm-cov that often report absolute paths.
    let path = Path::new(out_file);
    let abs = if path.is_absolute() {
        Some(path.to_path_buf())
    } else {
        std::env::current_dir().ok().map(|cwd| cwd.join(path))
    };
    if let Some(abs) = abs
        && let Ok(canon) = abs.canonicalize() {
        let canon = canon.to_string_lossy().into_owned();
        if canon != out_file && canon != norm
            && let Some(entry) = db.get_noweb_entry(&canon, out_line_0)? {
            return Ok(Some(entry));
        }
    }

    // Try 4: progressively strip leading path components and match by suffix.
    // This bridges current repo layouts like `crates/weaveback/src/main.rs`
    // to db keys like `weaveback/src/main.rs`.
    let components: Vec<_> = Path::new(out_file).components().collect();
    for i in 1..components.len() {
        let sub: std::path::PathBuf = components[i..].iter().collect();
        let sub_str = sub.to_string_lossy();
        if !sub_str.contains('/') {
            break;
        }
        if sub_str != out_file
            && sub_str != norm
            && let Some(entry) = db.get_noweb_entry_by_suffix(&sub_str, out_line_0)? {
            return Ok(Some(entry));
        }
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Confidence;
    use std::path::PathBuf;

    #[test]
    fn find_best_noweb_entry_can_match_by_suffix() {
        let mut db = WeavebackDb::open_temp().expect("temp db");
        db.set_noweb_entries(
            "/tmp/wb-pass-root/weaveback/src/main.rs",
            &[(
                99,
                NowebMapEntry {
                    src_file: "crates/weaveback/src/weaveback.adoc".to_string(),
                    chunk_name: "main command handler".to_string(),
                    src_line: 123,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("set noweb entries");

        let resolver = PathResolver::new(PathBuf::from("."), PathBuf::from("crates"));
        let entry = find_best_noweb_entry(
            &db,
            "crates/weaveback/src/main.rs",
            99,
            &resolver,
        )
        .expect("lookup ok")
        .expect("entry found");

        assert_eq!(entry.chunk_name, "main command handler");
        assert_eq!(entry.src_file, "crates/weaveback/src/weaveback.adoc");
    }

    #[test]
    fn db_suffix_lookup_prefers_shortest_matching_path() {
        let mut db = WeavebackDb::open_temp().expect("temp db");
        db.set_noweb_entries(
            "/tmp/a/weaveback/src/main.rs",
            &[(
                10,
                NowebMapEntry {
                    src_file: "a.adoc".to_string(),
                    chunk_name: "short".to_string(),
                    src_line: 1,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("set a");
        db.set_noweb_entries(
            "/tmp/very/long/prefix/weaveback/src/main.rs",
            &[(
                10,
                NowebMapEntry {
                    src_file: "b.adoc".to_string(),
                    chunk_name: "long".to_string(),
                    src_line: 2,
                    indent: String::new(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .expect("set b");

        let entry = db
            .get_noweb_entry_by_suffix("weaveback/src/main.rs", 10)
            .expect("lookup ok")
            .expect("entry found");

        assert_eq!(entry.chunk_name, "short");
        assert_eq!(entry.src_file, "a.adoc");
    }
}
