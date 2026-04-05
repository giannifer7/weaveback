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
mod tests {
    use super::{Workspace, WorkspaceConfig};
    use crate::change_plan::{ChangePlan, ChangeTarget, OutputAnchor, PlannedEdit};
    use std::path::PathBuf;

    fn sample_config() -> WorkspaceConfig {
        WorkspaceConfig {
            project_root: PathBuf::from("/tmp/weaveback-project"),
            db_path: PathBuf::from("/tmp/weaveback-project/weaveback.db"),
            gen_dir: PathBuf::from("/tmp/weaveback-project/gen"),
        }
    }

    fn sample_plan() -> ChangePlan {
        ChangePlan {
            plan_id: "plan-1".to_string(),
            goal: "test".to_string(),
            constraints: Vec::new(),
            edits: vec![PlannedEdit {
                edit_id: "edit-1".to_string(),
                rationale: "because".to_string(),
                target: ChangeTarget {
                    src_file: "project.adoc".to_string(),
                    src_line: 1,
                    src_line_end: 1,
                },
                new_src_lines: vec!["replacement".to_string()],
                anchor: OutputAnchor {
                    out_file: "gen/out.rs".to_string(),
                    out_line: 1,
                    expected_output: "replacement".to_string(),
                },
            }],
        }
    }

    #[test]
    fn workspace_session_preserves_config() {
        let workspace = Workspace::open(sample_config());
        let session = workspace.session();

        let err = session.search("needle", 5).unwrap_err();
        assert!(err.contains("Database not found at /tmp/weaveback-project/weaveback.db"));
    }

    #[test]
    fn validate_change_plan_uses_pure_validation_without_db() {
        let workspace = Workspace::open(sample_config());
        let session = workspace.session();
        let mut plan = sample_plan();
        plan.edits.push(plan.edits[0].clone());

        let validation = session.validate_change_plan(&plan).unwrap();
        assert!(!validation.ok);
        assert!(validation.issues.iter().any(|issue| issue.contains("duplicate edit_id")));
    }

    #[test]
    fn preview_change_plan_requires_db() {
        let workspace = Workspace::open(sample_config());
        let session = workspace.session();

        let err = session.preview_change_plan(&sample_plan()).unwrap_err();
        assert!(err.contains("Database not found at /tmp/weaveback-project/weaveback.db"));
    }

    #[test]
    fn apply_change_plan_requires_db() {
        let workspace = Workspace::open(sample_config());
        let session = workspace.session();

        let err = session.apply_change_plan(&sample_plan()).unwrap_err();
        assert!(err.contains("Database not found at /tmp/weaveback-project/weaveback.db"));
    }

    #[test]
    fn trace_rejects_zero_line_before_db_lookup() {
        let workspace = Workspace::open(sample_config());
        let session = workspace.session();

        let err = session.trace("gen/out.rs", 0, 1).unwrap_err();
        assert_eq!(err, "out_line must be >= 1");
    }
}
