use crate::change_plan::ChangePlan;
use crate::workspace::WorkspaceConfig;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use weaveback_core::PathResolver;
use weaveback_macro::evaluator::{EvalConfig, Evaluator};
use weaveback_macro::macro_api::process_string;
use weaveback_tangle::db::WeavebackDb;

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
    let db_path = resolver.normalize(request.out_file);
    let nw_entry = db
        .get_noweb_entry(&db_path, request.out_line_1 - 1)
        .map_err(|e| format!("db error: {e}"))?
        .ok_or_else(|| format!("No noweb map entry for {}:{}", request.out_file, request.out_line_1))?;

    let expanded_line_1 = nw_entry.src_line as usize + 1;
    let content = std::fs::read_to_string(request.src_file)
        .map_err(|e| format!("Cannot read {}: {e}", request.src_file))?;
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
        oracle_config.special_char = cfg.special_char;
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
        std::fs::write(request.src_file, &patched)
            .map_err(|e| format!("Cannot write {}: {e}", request.src_file))?;
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
