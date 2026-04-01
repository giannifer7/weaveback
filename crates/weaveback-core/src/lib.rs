/// Maximum recursion depth for macro expansion and noweb chunk expansion.
pub const MAX_RECURSION_DEPTH: usize = 100;
use std::path::{Path, PathBuf};

pub struct PathResolver {
    project_root: PathBuf,
    gen_dir: PathBuf,
}

impl PathResolver {
    pub fn new(project_root: PathBuf, gen_dir: PathBuf) -> Self {
        Self { project_root, gen_dir }
    }

    /// Convert any representation of a generated file path into the
    /// db key form (relative to gen_dir, no leading `./`).
    pub fn normalize(&self, input: &str) -> String {
        let path = Path::new(input);

        // 1. Strip gen_dir prefix (relative path case).
        if let Ok(rel) = path.strip_prefix(&self.gen_dir) {
            return rel.to_string_lossy().into_owned();
        }

        // 2. Strip project_root/gen_dir prefix (absolute path case).
        if let Ok(rel) = path.strip_prefix(self.project_root.join(&self.gen_dir)) {
            return rel.to_string_lossy().into_owned();
        }

        // 3. String-based fallback (covers platforms where Path::strip_prefix
        //    is stricter about trailing separators).
        let gen_str = self.gen_dir.to_string_lossy();
        if gen_str != "." {
            let prefix = format!("{}/", gen_str);
            if let Some(rest) = input.strip_prefix(prefix.as_str()) {
                return rest.to_string();
            }
        }

        // 4. Already a db key: strip any leading "./".
        input.trim_start_matches("./").to_string()
    }

    /// Resolve a source-file path from the database to a concrete disk path.
    /// Paths in the db are relative to `project_root`; if the path does not
    /// exist relative to the current directory, fall back to `project_root`.
    pub fn resolve_src(&self, src_file: &str) -> PathBuf {
        let p = Path::new(src_file);
        if p.is_absolute() || p.exists() {
            p.to_path_buf()
        } else {
            self.project_root.join(p)
        }
    }

    /// Resolve a db key back to the absolute path of the generated file.
    pub fn resolve_gen(&self, db_path: &str) -> PathBuf {
        self.project_root.join(&self.gen_dir).join(db_path)
    }
}
