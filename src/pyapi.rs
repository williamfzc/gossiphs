/*
File: pyapi.rs
Functionality: Python-specific API functions.
Role: Provides direct wrappers for core functionality to be called from Python via PyO3.
*/
use crate::graph::{Graph, GraphConfig};
use pyo3::prelude::*;

#[pyfunction]
pub fn create_graph(config: GraphConfig) -> PyResult<Graph> {
    let g = Graph::from(config);
    Ok(g)
}
