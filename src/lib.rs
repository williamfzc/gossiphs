pub mod api;
pub(crate) mod extractor;
pub mod graph;
mod rule;
pub mod server;
pub mod symbol;

// py wrapper
use crate::graph::{Graph, GraphConfig};
use pyo3::prelude::*;

mod pyapi;

#[pymodule]
fn _rust_api(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(pyapi::create_graph, m)?)?;
    m.add_class::<GraphConfig>()?;
    m.add_class::<Graph>()?;
    Ok(())
}
