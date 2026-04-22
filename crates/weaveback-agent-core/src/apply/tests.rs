// weaveback-agent-core/src/apply/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::{apply_change_plan, preview_change_plan, sorted_plan_edit_ids, validate_change_plan};
use crate::change_plan::{ChangePlan, ChangeTarget, OutputAnchor, PlannedEdit};
use crate::workspace::WorkspaceConfig;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use weaveback_tangle::db::{Confidence, NowebMapEntry, TangleConfig, WeavebackDb};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TestWorkspace {
    root: PathBuf,
    db_path: PathBuf,
    gen_dir: PathBuf,
}

impl TestWorkspace {
    fn new() -> Self {
        let unique = format!(
            "wb-agent-apply-{}-{}",
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
        Self {
            root,
            db_path,
            gen_dir,
        }
    }

    fn config(&self) -> WorkspaceConfig {
        WorkspaceConfig {
            project_root: self.root.clone(),
            db_path: self.db_path.clone(),
            gen_dir: self.gen_dir.clone(),
        }
    }

    fn write_source(&self, rel: &str, content: &str) -> String {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create source parent");
        }
        fs::write(&path, content).expect("write source");
        path.to_string_lossy().into_owned()
    }

    fn read_source(&self, rel: &str) -> String {
        fs::read_to_string(self.root.join(rel)).expect("read source")
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

struct TestPlanSpec<'a> {
    src_file: &'a str,
    edit_id: &'a str,
    src_line: usize,
    src_line_end: usize,
    new_src_lines: &'a [&'a str],
    out_file: &'a str,
    out_line: u32,
    expected_output: &'a str,
}

fn simple_plan(spec: TestPlanSpec<'_>) -> ChangePlan {
    ChangePlan {
        plan_id: "plan-1".to_string(),
        goal: "test".to_string(),
        constraints: Vec::new(),
        edits: vec![PlannedEdit {
            edit_id: spec.edit_id.to_string(),
            rationale: "because".to_string(),
            target: ChangeTarget {
                src_file: spec.src_file.to_string(),
                src_line: spec.src_line,
                src_line_end: spec.src_line_end,
            },
            new_src_lines: spec
                .new_src_lines
                .iter()
                .map(|line| (*line).to_string())
                .collect(),
            anchor: OutputAnchor {
                out_file: spec.out_file.to_string(),
                out_line: spec.out_line,
                expected_output: spec.expected_output.to_string(),
            },
        }],
    }
}

fn install_literal_fixture(workspace: &TestWorkspace, rel: &str, content: &str, out_file: &str) -> String {
    let src_path = workspace.write_source(rel, content);
    let mut db = workspace.open_db();
    db.set_src_snapshot(rel, content.as_bytes()).unwrap();
    db.set_source_config(
        rel,
        &TangleConfig {
            sigil: '%',
            open_delim: "<<".to_string(),
            close_delim: ">>".to_string(),
            chunk_end: "@".to_string(),
            comment_markers: vec!["//".to_string()],
        },
    )
    .unwrap();
    db.set_noweb_entries(
        out_file,
        &[(
            0,
            NowebMapEntry {
                src_file: rel.to_string(),
                chunk_name: "literal".to_string(),
                src_line: 0,
                indent: String::new(),
                confidence: Confidence::Exact,
            },
        )],
    )
    .unwrap();
    src_path
}

#[test]
fn sorted_plan_edit_ids_orders_by_file_then_reverse_line() {
    let plan = ChangePlan {
        plan_id: "plan".to_string(),
        goal: "goal".to_string(),
        constraints: Vec::new(),
        edits: vec![
            simple_plan(TestPlanSpec { src_file: "b.adoc", edit_id: "b-low", src_line: 1, src_line_end: 1, new_src_lines: &["b"], out_file: "gen/b", out_line: 1, expected_output: "b" }).edits.remove(0),
            simple_plan(TestPlanSpec { src_file: "a.adoc", edit_id: "a-low", src_line: 2, src_line_end: 2, new_src_lines: &["a2"], out_file: "gen/a", out_line: 1, expected_output: "a2" }).edits.remove(0),
            simple_plan(TestPlanSpec { src_file: "a.adoc", edit_id: "a-high", src_line: 5, src_line_end: 6, new_src_lines: &["a5"], out_file: "gen/a", out_line: 1, expected_output: "a5" }).edits.remove(0),
        ],
    };

    let order = sorted_plan_edit_ids(&plan);
    let ids: Vec<&str> = order.iter().map(|idx| plan.edits[*idx].edit_id.as_str()).collect();
    assert_eq!(ids, vec!["a-high", "a-low", "b-low"]);
}

#[test]
fn validate_change_plan_reports_empty_invalid_and_overlapping_edits() {
    let mut plan = ChangePlan {
        plan_id: "plan".to_string(),
        goal: "goal".to_string(),
        constraints: Vec::new(),
        edits: Vec::new(),
    };
    let validation = validate_change_plan(
        &WorkspaceConfig {
            project_root: PathBuf::new(),
            db_path: PathBuf::new(),
            gen_dir: PathBuf::new(),
        },
        &plan,
    )
    .unwrap();
    assert!(!validation.ok);
    assert!(validation.issues.iter().any(|issue| issue.contains("at least one edit")));

    plan.edits = vec![
        PlannedEdit {
            edit_id: "dup".to_string(),
            rationale: "x".to_string(),
            target: ChangeTarget {
                src_file: "a.adoc".to_string(),
                src_line: 4,
                src_line_end: 3,
            },
            new_src_lines: vec!["x".to_string()],
            anchor: OutputAnchor {
                out_file: "gen/a".to_string(),
                out_line: 0,
                expected_output: "x".to_string(),
            },
        },
        PlannedEdit {
            edit_id: "dup".to_string(),
            rationale: "y".to_string(),
            target: ChangeTarget {
                src_file: "a.adoc".to_string(),
                src_line: 3,
                src_line_end: 5,
            },
            new_src_lines: vec!["y".to_string()],
            anchor: OutputAnchor {
                out_file: "gen/a".to_string(),
                out_line: 1,
                expected_output: "y".to_string(),
            },
        },
    ];

    let validation = validate_change_plan(
        &WorkspaceConfig {
            project_root: PathBuf::new(),
            db_path: PathBuf::new(),
            gen_dir: PathBuf::new(),
        },
        &plan,
    )
    .unwrap();
    assert!(!validation.ok);
    assert!(validation.issues.iter().any(|issue| issue.contains("duplicate edit_id")));
    assert!(validation.issues.iter().any(|issue| issue.contains("invalid source range")));
    assert!(validation.issues.iter().any(|issue| issue.contains("invalid output anchor")));
    assert!(validation.issues.iter().any(|issue| issue.contains("overlapping edits")));
}

#[test]
fn preview_change_plan_reports_oracle_success_without_writing() {
    let workspace = TestWorkspace::new();
    let src_path = install_literal_fixture(&workspace, "docs/sample.adoc", "before\n", "gen/out.txt");
    let plan = simple_plan(TestPlanSpec {
        src_file: &src_path,
        edit_id: "edit-1",
        src_line: 1,
        src_line_end: 1,
        new_src_lines: &["after"],
        out_file: "gen/out.txt",
        out_line: 1,
        expected_output: "after",
    });

    let preview = preview_change_plan(&workspace.config(), &plan).unwrap();
    assert_eq!(preview.plan_id, "plan-1");
    assert_eq!(preview.edits.len(), 1);
    assert_eq!(preview.edits[0].edit_id, "edit-1");
    assert!(preview.edits[0].oracle_ok);
    assert_eq!(preview.edits[0].src_before, vec!["before"]);
    assert_eq!(preview.edits[0].src_after, vec!["after"]);
    assert_eq!(workspace.read_source("docs/sample.adoc"), "before\n");
}

#[test]
fn preview_change_plan_reports_oracle_failure() {
    let workspace = TestWorkspace::new();
    let src_path = install_literal_fixture(&workspace, "docs/sample.adoc", "before\n", "gen/out.txt");
    let plan = simple_plan(TestPlanSpec {
        src_file: &src_path,
        edit_id: "edit-1",
        src_line: 1,
        src_line_end: 1,
        new_src_lines: &["after"],
        out_file: "gen/out.txt",
        out_line: 1,
        expected_output: "expected something else",
    });

    let err = preview_change_plan(&workspace.config(), &plan).unwrap_err();
    assert!(err.contains("Oracle check failed"));
    assert!(err.contains("expected something else"));
    assert_eq!(workspace.read_source("docs/sample.adoc"), "before\n");
}

#[test]
fn apply_change_plan_writes_successful_edits_and_reports_failures() {
    let workspace = TestWorkspace::new();
    let src_path = install_literal_fixture(&workspace, "docs/sample.adoc", "before\nkeep\n", "gen/out.txt");
    let success = simple_plan(TestPlanSpec {
        src_file: &src_path,
        edit_id: "ok",
        src_line: 1,
        src_line_end: 1,
        new_src_lines: &["after"],
        out_file: "gen/out.txt",
        out_line: 1,
        expected_output: "after",
    })
        .edits
        .remove(0);
    let failure = simple_plan(TestPlanSpec {
        src_file: &src_path,
        edit_id: "bad",
        src_line: 2,
        src_line_end: 2,
        new_src_lines: &["broken"],
        out_file: "gen/out.txt",
        out_line: 2,
        expected_output: "nope",
    })
    .edits
    .remove(0);

    let plan = ChangePlan {
        plan_id: "plan-apply".to_string(),
        goal: "goal".to_string(),
        constraints: Vec::new(),
        edits: vec![success, failure],
    };

    let result = apply_change_plan(&workspace.config(), &plan).unwrap();
    assert!(!result.applied);
    assert_eq!(result.plan_id, "plan-apply");
    assert_eq!(result.applied_edit_ids, vec!["ok"]);
    assert_eq!(result.failed_edit_ids, vec!["bad"]);
    assert_eq!(workspace.read_source("docs/sample.adoc"), "after\nkeep\n");
}

