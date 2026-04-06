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

#[cfg(test)]
mod tests {
    use super::{find_best_noweb_entry, find_best_source_config, find_line_col};
    use crate::db::{Confidence, NowebMapEntry, TangleConfig, WeavebackDb};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    use weaveback_core::PathResolver;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestWorkspace {
        root: PathBuf,
        gen_dir: PathBuf,
        db_path: PathBuf,
    }

    impl TestWorkspace {
        fn new() -> Self {
            let unique = format!(
                "wb-lookup-tests-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock drifted backwards")
                    .as_nanos()
                    + u128::from(TEST_COUNTER.fetch_add(1, Ordering::Relaxed))
            );
            let root = std::env::temp_dir().join(unique);
            let gen_dir = root.join("gen");
            let db_path = root.join("weaveback.db");
            fs::create_dir_all(&gen_dir).expect("create temp workspace");
            Self { root, gen_dir, db_path }
        }

        fn resolver(&self) -> PathResolver {
            PathResolver::new(self.root.clone(), self.gen_dir.clone())
        }

        fn open_db(&self) -> WeavebackDb {
            WeavebackDb::open(&self.db_path).expect("open sqlite db")
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn sample_config() -> TangleConfig {
        TangleConfig {
            sigil: '%',
            open_delim: "<<".to_string(),
            close_delim: ">>".to_string(),
            chunk_end: "@".to_string(),
            comment_markers: vec!["//".to_string()],
        }
    }

    #[test]
    fn find_line_col_counts_utf8_columns() {
        let text = "alpha\nbèta\n終";
        assert_eq!(find_line_col(text, 0), (1, 1));
        assert_eq!(find_line_col(text, 6), (2, 1));
        assert_eq!(find_line_col(text, "alpha\nbè".len()), (2, 3));
        assert_eq!(find_line_col(text, text.len()), (3, 2));
    }

    #[test]
    fn find_best_noweb_entry_uses_exact_and_normalized_paths() {
        let workspace = TestWorkspace::new();
        let mut db = workspace.open_db();
        db.set_noweb_entries(
            "out.rs",
            &[(
                0,
                NowebMapEntry {
                    src_file: "docs/source.adoc".to_string(),
                    chunk_name: "alpha".to_string(),
                    src_line: 10,
                    indent: "    ".to_string(),
                    confidence: Confidence::Exact,
                },
            )],
        )
        .unwrap();

        let resolver = workspace.resolver();
        let exact = find_best_noweb_entry(&db, "out.rs", 0, &resolver).unwrap().unwrap();
        assert_eq!(exact.chunk_name, "alpha");

        let gen_path = workspace.gen_dir.join("out.rs");
        let normalized = find_best_noweb_entry(&db, gen_path.to_string_lossy().as_ref(), 0, &resolver)
            .unwrap()
            .unwrap();
        assert_eq!(normalized.src_file, "docs/source.adoc");
    }

    #[test]
    fn find_best_source_config_handles_exact_dotslash_prefix_and_absolute_name() {
        let workspace = TestWorkspace::new();
        let db = workspace.open_db();
        let cfg = sample_config();

        db.set_source_config("docs/exact.adoc", &cfg).unwrap();
        assert!(find_best_source_config(&db, "docs/exact.adoc").unwrap().is_some());
        assert!(find_best_source_config(&db, "./docs/exact.adoc").unwrap().is_some());

        db.set_source_config("./docs/dotted.adoc", &cfg).unwrap();
        assert!(find_best_source_config(&db, "docs/dotted.adoc").unwrap().is_some());

        db.set_source_config("lookup.adoc", &cfg).unwrap();
        let abs = workspace.root.join("nested").join("lookup.adoc");
        assert!(find_best_source_config(&db, abs.to_string_lossy().as_ref()).unwrap().is_some());
    }

    #[test]
    fn find_best_source_config_strips_crates_prefix() {
        let workspace = TestWorkspace::new();
        let db = workspace.open_db();
        let cfg = sample_config();

        db.set_source_config("weaveback/src/main.adoc", &cfg).unwrap();
        let found = find_best_source_config(&db, "crates/weaveback/src/main.adoc").unwrap();
        assert!(found.is_some());
    }
}
