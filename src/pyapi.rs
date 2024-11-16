use crate::graph::{Graph, GraphConfig};
use pyo3::prelude::*;

#[pyfunction]
pub fn create_graph(config: GraphConfig) -> Graph {
    let g = Graph::from(config);
    g
}
