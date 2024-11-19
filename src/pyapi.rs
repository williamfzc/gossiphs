use crate::graph::{Graph, GraphConfig};
use pyo3::prelude::*;

#[pyfunction]
pub fn create_graph(config: GraphConfig) -> PyResult<Graph> {
    let g = Graph::from(config);
    Ok(g)
}
