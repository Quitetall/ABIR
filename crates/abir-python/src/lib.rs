use abir_core::VERSION;
use pyo3::prelude::*;

#[pyfunction]
fn version() -> &'static str {
    VERSION
}

#[pymodule]
fn abir(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(version, module)?)?;
    Ok(())
}
