// weaveback-py/src/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use tempfile::tempdir;

#[test]
fn test_py_workspace_basic() {
    Python::initialize();
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("wb.db");
    let gen_dir = tmp.path().join("gen");
    std::fs::create_dir_all(&gen_dir).unwrap();

    // Initialize an empty database so Workspace::open doesn't fail
    weaveback_tangle::db::WeavebackDb::open(&db_path).unwrap();

    Python::attach(|py| {
        let ws = PyWorkspace::new(
            tmp.path().to_string_lossy().to_string(),
            db_path.to_string_lossy().to_string(),
            gen_dir.to_string_lossy().to_string()
        );

        // Search (empty DB should return empty list or empty result)
        let res = ws.search(py, "test", 10).unwrap();
        assert!(res.bind(py).is_instance_of::<pyo3::types::PyList>());

        // Trace
        let res = ws.trace(py, "nonexistent.rs", 1, 1).unwrap();
        assert!(res.bind(py).is_none() || res.bind(py).is_instance_of::<pyo3::types::PyDict>());

        // Change plan methods (validate, preview, apply)
        // Use pythonize to create a valid ChangePlan from Rust
        let plan = weaveback_agent_core::ChangePlan {
            plan_id: "test-plan".to_string(),
            goal: "test-goal".to_string(),
            constraints: vec![],
            edits: vec![weaveback_agent_core::PlannedEdit {
                edit_id: "e1".to_string(),
                rationale: "r1".to_string(),
                target: weaveback_agent_core::ChangeTarget {
                    src_file: "test.adoc".to_string(),
                    src_line: 1,
                    src_line_end: 2,
                },
                new_src_lines: vec!["line1".to_string()],
                anchor: weaveback_agent_core::OutputAnchor {
                    out_file: "test.rs".to_string(),
                    out_line: 1,
                    expected_output: "old".to_string(),
                },
            }],
        };

        let plan_any = pythonize(py, &plan).unwrap();

        let _ = ws.validate_change_plan(py, plan_any.clone());
        let _ = ws.preview_change_plan(py, plan_any.clone());
        let _ = ws.apply_change_plan(py, plan_any.clone());
    });
}

