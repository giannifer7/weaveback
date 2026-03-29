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

    pub fn normalize(&self, input: &str) -> String {
        let path = Path::new(input);

        // 1. If gen_dir is ".", we just strip "./" and leading slashes.
        if self.gen_dir == Path::new(".") {
            return input.trim_start_matches("./").trim_start_matches('/').to_string();
        }

        // 2. Try stripping gen_dir from absolute or root-relative path.
        if let Ok(rel) = path.strip_prefix(&self.gen_dir) {
            return rel.to_string_lossy().into_owned();
        }

        // 3. Try stripping project_root/gen_dir.
        let full_gen = self.project_root.join(&self.gen_dir);
        if let Ok(rel) = path.strip_prefix(&full_gen) {
            return rel.to_string_lossy().into_owned();
        }

        // 4. Try stripping workspace members prefix (e.g. "crates/").
        for prefix in &["crates/", "./crates/"] {
            if let Some(stripped) = input.strip_prefix(prefix) {
                let stripped_path = Path::new(stripped);
                if let Ok(rel) = stripped_path.strip_prefix(&self.gen_dir) {
                    return rel.to_string_lossy().into_owned();
                }
                // Also try if the gen_dir itself was relative to the crate.
                let gen_str = self.gen_dir.to_string_lossy();
                if let Some(final_path) = stripped.strip_prefix(gen_str.as_ref()) {
                    return final_path.trim_start_matches('/').to_string();
                }
            }
        }

        // 5. If the path starts with the gen_dir as a string, strip it manually.
        let gen_str = self.gen_dir.to_string_lossy();
        let gen_prefix = if gen_str.ends_with('/') { gen_str.to_string() } else { format!("{}/", gen_str) };
        if input.starts_with(&gen_prefix) {
            return input[gen_prefix.len()..].to_string();
        }

        // 6. Fallback: if it's already relative and doesn't start with gen_dir,
        // it might already be in the DB format.
        input.trim_start_matches("./").to_string()
    }


    /// Resolves a database path (relative to gen_dir) back to an absolute disk path.
    pub fn resolve_gen(&self, db_path: &str) -> PathBuf {
        self.project_root.join(&self.gen_dir).join(db_path)
    }
}
