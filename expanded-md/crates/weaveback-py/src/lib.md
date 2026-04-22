# weaveback-py — Python bindings

PyO3-based Python extension module `_weaveback`.  Exposes the
`weaveback-agent-core` `Workspace` API to Python callers.

The `cdylib` is built with `maturin` or `cargo build` and imported
as `import _weaveback` from Python.

## `PyWorkspace` class

Wraps a `Workspace` instance.  All methods return JSON-serialisable
Python objects via `pythonize`.


```rust
// <[@file weaveback-py/src/lib.rs]>=
// weaveback-py/src/lib.rs
// I'd Really Rather You Didn't edit this generated file.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pythonize::{depythonize, pythonize};
use weaveback_agent_core::{ChangePlan, Workspace, WorkspaceConfig};

#[pyclass]
struct PyWorkspace {
    inner: Workspace,
}

#[pymethods]
impl PyWorkspace {
    #[new]
    fn new(project_root: String, db_path: String, gen_dir: String) -> Self {
        let config = WorkspaceConfig {
            project_root: project_root.into(),
            db_path: db_path.into(),
            gen_dir: gen_dir.into(),
        };

        Self {
            inner: Workspace::open(config),
        }
    }

    fn search(&self, py: Python<'_>, query: &str, limit: usize) -> PyResult<Py<PyAny>> {
        let value = self.inner.session().search(query, limit)
            .map_err(PyRuntimeError::new_err)?;
        pythonize(py, &value)
            .map(|value| value.unbind())
            .map_err(Into::into)
    }

    fn trace(&self, py: Python<'_>, out_file: &str, out_line: u32, out_col: u32) -> PyResult<Py<PyAny>> {
        let value = self.inner.session().trace(out_file, out_line, out_col)
            .map_err(PyRuntimeError::new_err)?;
        pythonize(py, &value)
            .map(|value| value.unbind())
            .map_err(Into::into)
    }

    fn validate_change_plan(&self, py: Python<'_>, plan: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let plan: ChangePlan = depythonize(&plan)?;
        let value = self.inner.session().validate_change_plan(&plan)
            .map_err(PyRuntimeError::new_err)?;
        pythonize(py, &value)
            .map(|value| value.unbind())
            .map_err(Into::into)
    }

    fn preview_change_plan(&self, py: Python<'_>, plan: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let plan: ChangePlan = depythonize(&plan)?;
        let value = self.inner.session().preview_change_plan(&plan)
            .map_err(PyRuntimeError::new_err)?;
        pythonize(py, &value)
            .map(|value| value.unbind())
            .map_err(Into::into)
    }

    fn apply_change_plan(&self, py: Python<'_>, plan: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let plan: ChangePlan = depythonize(&plan)?;
        let value = self.inner.session().apply_change_plan(&plan)
            .map_err(PyRuntimeError::new_err)?;
        pythonize(py, &value)
            .map(|value| value.unbind())
            .map_err(Into::into)
    }
}

#[pymodule]
fn _weaveback(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyWorkspace>()?;
    Ok(())
}
#[cfg(test)]
mod tests;

// @
```



```rust
// <[@file weaveback-py/src/tests.rs]>=
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

// @
```

