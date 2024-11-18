pub(crate) mod extractor;
pub mod api;
pub mod graph;
mod rule;
pub mod server;
pub mod symbol;

// py wrapper
use pyo3::prelude::*;
use crate::graph::GraphConfig;

mod pyapi;

#[pymodule]
fn _rust_api(m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(pyapi::create_graph, m)?)?;
    m.add_class::<GraphConfig>()?;
    Ok(())
}
