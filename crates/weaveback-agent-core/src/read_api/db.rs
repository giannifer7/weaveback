// weaveback-agent-core/src/read_api/db.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub(in crate::read_api) fn open_db(config: &WorkspaceConfig) -> Result<WeavebackDb, String> {
    if !config.db_path.exists() {
        return Err(format!(
            "Database not found at {}. Run weaveback on your source files first.",
            config.db_path.display()
        ));
    }
    WeavebackDb::open_read_only(&config.db_path).map_err(|e| e.to_string())
}

pub(in crate::read_api) fn build_eval_config() -> EvalConfig {
    EvalConfig::default()
}

