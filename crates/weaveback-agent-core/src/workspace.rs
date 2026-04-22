// weaveback-agent-core/src/workspace.rs
// I'd Really Rather You Didn't edit this generated file.

use crate::apply::{apply_change_plan, preview_change_plan, validate_change_plan, ApplyResult, ChangePreview, PlanValidation};
use crate::change_plan::ChangePlan;
use crate::read_api::{ChunkContext, SearchHit, TraceResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub project_root: PathBuf,
    pub db_path: PathBuf,
    pub gen_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    config: WorkspaceConfig,
}

#[derive(Debug, Clone)]
pub struct Session {
    config: WorkspaceConfig,
}

impl Workspace {
    pub fn open(config: WorkspaceConfig) -> Self {
        Self { config }
    }

    pub fn session(&self) -> Session {
        Session {
            config: self.config.clone(),
        }
    }
}

impl Session {
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, String> {
        crate::read_api::search(&self.config, query, limit)
    }

    pub fn trace(&self, out_file: &str, out_line: u32, out_col: u32) -> Result<Option<TraceResult>, String> {
        crate::read_api::trace(&self.config, out_file, out_line, out_col)
    }

    pub fn chunk_context(&self, file: &str, name: &str, nth: u32) -> Result<ChunkContext, String> {
        crate::read_api::chunk_context(&self.config, file, name, nth)
    }

    pub fn validate_change_plan(&self, plan: &ChangePlan) -> Result<PlanValidation, String> {
        validate_change_plan(&self.config, plan)
    }

    pub fn preview_change_plan(&self, plan: &ChangePlan) -> Result<ChangePreview, String> {
        preview_change_plan(&self.config, plan)
    }

    pub fn apply_change_plan(&self, plan: &ChangePlan) -> Result<ApplyResult, String> {
        apply_change_plan(&self.config, plan)
    }
}
#[cfg(test)]
mod tests;

