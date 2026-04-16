use crate::change_plan::ChangePlan;
use crate::workspace::WorkspaceConfig;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use weaveback_core::PathResolver;
use weaveback_macro::evaluator::{EvalConfig, Evaluator};
use weaveback_macro::macro_api::process_string;
use weaveback_tangle::db::WeavebackDb;
use weaveback_tangle::lookup::find_best_noweb_entry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanValidation {
    pub ok: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewedEdit {
    pub edit_id: String,
    pub oracle_ok: bool,
    pub src_before: Vec<String>,
    pub src_after: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePreview {
    pub plan_id: String,
    pub edits: Vec<PreviewedEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyResult {
    pub plan_id: String,
    pub applied: bool,
    pub applied_edit_ids: Vec<String>,
    pub failed_edit_ids: Vec<String>,
}

struct ApplyFixRequest<'a> {
    src_file: &'a str,
    src_line_1: usize,
    src_line_end_1: usize,
    new_lines: &'a [String],
    out_file: &'a str,
    out_line_1: u32,
    expected: &'a str,
    write_changes: bool,
}

fn open_db(config: &WorkspaceConfig) -> Result<WeavebackDb, String> {
    if !config.db_path.exists() {
        return Err(format!(
            "Database not found at {}. Run weaveback on your source files first.",
            config.db_path.display()
        ));
    }
    WeavebackDb::open_read_only(&config.db_path).map_err(|e| e.to_string())
}

fn build_eval_config() -> EvalConfig {
    EvalConfig::default()
}

fn sorted_plan_edit_ids(plan: &ChangePlan) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..plan.edits.len()).collect();
    indices.sort_by(|left, right| {
        let left_edit = &plan.edits[*left];
        let right_edit = &plan.edits[*right];
        left_edit
            .target
            .src_file
            .cmp(&right_edit.target.src_file)
            .then_with(|| right_edit.target.src_line.cmp(&left_edit.target.src_line))
            .then_with(|| right_edit.target.src_line_end.cmp(&left_edit.target.src_line_end))
    });
    indices
}

fn apply_fix_impl(
    request: ApplyFixRequest<'_>,
    db: &WeavebackDb,
    resolver: &PathResolver,
    eval_config: &EvalConfig,
) -> Result<PreviewedEdit, String> {
    let nw_entry = find_best_noweb_entry(db, request.out_file, request.out_line_1 - 1, resolver)
        .map_err(|e| format!("db error: {e}"))?
        .ok_or_else(|| format!("No noweb map entry for {}:{}", request.out_file, request.out_line_1))?;

    let expanded_line_1 = nw_entry.src_line as usize + 1;
    let full_src_path = resolver.resolve_src(request.src_file);
    let content = std::fs::read_to_string(&full_src_path)
        .map_err(|e| format!("Cannot read {}: {e}", full_src_path.display()))?;
    let orig_lines: Vec<&str> = content.lines().collect();
    let file_len = orig_lines.len();

    if request.src_line_1 == 0 || request.src_line_1 > file_len {
        return Err(format!("src_line {} out of range (file has {file_len} lines)", request.src_line_1));
    }
    if request.src_line_end_1 > file_len {
        return Err(format!("src_line_end {} out of range (file has {file_len} lines)", request.src_line_end_1));
    }
    if request.src_line_end_1 < request.src_line_1 {
        return Err("src_line_end must be >= src_line".to_string());
    }

    let lo = request.src_line_1 - 1;
    let hi = request.src_line_end_1 - 1;
    let src_before: Vec<String> = orig_lines[lo..=hi].iter().map(|line| (*line).to_string()).collect();

    let patched_lines: Vec<&str> = orig_lines[..lo]
        .iter().copied()
        .chain(request.new_lines.iter().map(String::as_str))
        .chain(orig_lines[hi + 1..].iter().copied())
        .collect();

    let had_trailing_newline = content.ends_with('\n');
    let mut patched = patched_lines.join("\n");
    if had_trailing_newline {
        patched.push('\n');
    }

    let oracle_path = std::path::Path::new(request.src_file).with_file_name("<oracle>");
    let mut oracle_config = eval_config.clone();
    if let Ok(Some(cfg)) = weaveback_tangle::lookup::find_best_source_config(db, request.src_file) {
        oracle_config.sigil = cfg.sigil;
    }

    let mut evaluator = Evaluator::new(oracle_config);
    let expanded_bytes = process_string(&patched, Some(&oracle_path), &mut evaluator)
        .map_err(|e| format!("Evaluation error: {e:?}"))?;
    let expanded = String::from_utf8_lossy(&expanded_bytes);

    let actual_line = expanded.lines().nth(expanded_line_1 - 1)
        .ok_or_else(|| format!("Expanded output has fewer than {expanded_line_1} lines"))?;

    if actual_line != request.expected {
        return Err(format!(
            "Oracle check failed for {}:{}-{}: got {:?}, expected {:?}",
            request.src_file, request.src_line_1, request.src_line_end_1, actual_line, request.expected
        ));
    }

    if request.write_changes {
        std::fs::write(&full_src_path, &patched)
            .map_err(|e| format!("Cannot write {}: {e}", full_src_path.display()))?;
    }

    Ok(PreviewedEdit {
        edit_id: String::new(),
        oracle_ok: true,
        src_before,
        src_after: request.new_lines.to_vec(),
    })
}

pub fn validate_change_plan(_config: &WorkspaceConfig, plan: &ChangePlan) -> Result<PlanValidation, String> {
    let mut issues = Vec::new();
    let mut ids = HashSet::new();
    let mut per_file_ranges: BTreeMap<&str, Vec<(usize, usize, &str)>> = BTreeMap::new();

    if plan.edits.is_empty() {
        issues.push("plan must contain at least one edit".to_string());
    }

    for edit in &plan.edits {
        if !ids.insert(edit.edit_id.as_str()) {
            issues.push(format!("duplicate edit_id: {}", edit.edit_id));
        }
        if edit.target.src_line == 0 || edit.target.src_line_end < edit.target.src_line {
            issues.push(format!("{} has an invalid source range", edit.edit_id));
        }

        if edit.anchor.out_line == 0 {
            issues.push(format!("{} has an invalid output anchor", edit.edit_id));
        }

        per_file_ranges
            .entry(edit.target.src_file.as_str())
            .or_default()
            .push((edit.target.src_line, edit.target.src_line_end, edit.edit_id.as_str()));
    }

    for (src_file, ranges) in &mut per_file_ranges {
        ranges.sort_by_key(|(start, end, _)| (*start, *end));
        for pair in ranges.windows(2) {
            let (left_start, left_end, left_id) = pair[0];
            let (right_start, right_end, right_id) = pair[1];
            if right_start <= left_end {
                issues.push(format!(
                    "overlapping edits in {}: {} ({}-{}) overlaps {} ({}-{})",
                    src_file, left_id, left_start, left_end, right_id, right_start, right_end
                ));
            }
        }
    }

    Ok(PlanValidation {
        ok: issues.is_empty(),
        issues,
    })
}

pub fn preview_change_plan(config: &WorkspaceConfig, plan: &ChangePlan) -> Result<ChangePreview, String> {
    let validation = validate_change_plan(config, plan)?;
    if !validation.ok {
        return Err(validation.issues.join("\n"));
    }

    let db = open_db(config)?;
    let resolver = PathResolver::new(config.project_root.clone(), config.gen_dir.clone());
    let eval_config = build_eval_config();
    let mut previews = Vec::new();

    for idx in sorted_plan_edit_ids(plan) {
        let edit = &plan.edits[idx];
        let mut preview = apply_fix_impl(
            ApplyFixRequest {
                src_file: &edit.target.src_file,
                src_line_1: edit.target.src_line,
                src_line_end_1: edit.target.src_line_end,
                new_lines: &edit.new_src_lines,
                out_file: &edit.anchor.out_file,
                out_line_1: edit.anchor.out_line,
                expected: &edit.anchor.expected_output,
                write_changes: false,
            },
            &db,
            &resolver,
            &eval_config,
        )?;
        preview.edit_id = edit.edit_id.clone();
        previews.push(preview);
    }

    Ok(ChangePreview {
        plan_id: plan.plan_id.clone(),
        edits: previews,
    })
}

pub fn apply_change_plan(config: &WorkspaceConfig, plan: &ChangePlan) -> Result<ApplyResult, String> {
    let validation = validate_change_plan(config, plan)?;
    if !validation.ok {
        return Err(validation.issues.join("\n"));
    }

    let db = open_db(config)?;
    let resolver = PathResolver::new(config.project_root.clone(), config.gen_dir.clone());
    let eval_config = build_eval_config();
    let mut applied_edit_ids = Vec::new();
    let mut failed_edit_ids = Vec::new();

    for idx in sorted_plan_edit_ids(plan) {
        let edit = &plan.edits[idx];
        match apply_fix_impl(
            ApplyFixRequest {
                src_file: &edit.target.src_file,
                src_line_1: edit.target.src_line,
                src_line_end_1: edit.target.src_line_end,
                new_lines: &edit.new_src_lines,
                out_file: &edit.anchor.out_file,
                out_line_1: edit.anchor.out_line,
                expected: &edit.anchor.expected_output,
                write_changes: true,
            },
            &db,
            &resolver,
            &eval_config,
        ) {
            Ok(_) => applied_edit_ids.push(edit.edit_id.clone()),
            Err(_) => failed_edit_ids.push(edit.edit_id.clone()),
        }
    }

    Ok(ApplyResult {
        plan_id: plan.plan_id.clone(),
        applied: failed_edit_ids.is_empty(),
        applied_edit_ids,
        failed_edit_ids,
    })
}

#[cfg(test)]
mod tests {
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
}
