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

