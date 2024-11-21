pub mod api;
pub(crate) mod extractor;
pub mod graph;
mod rule;
pub mod server;
pub mod symbol;

// py wrapper
use crate::graph::{Graph, GraphConfig, RelatedSymbol};
use pyo3::prelude::*;

mod pyapi;

use crate::symbol::{DefRefPair, Symbol};
use pyo3_stub_gen::define_stub_info_gatherer;
use crate::api::{FileMetadata, RelatedFileContext};

#[pymodule]
fn _rust_api(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(pyapi::create_graph, m)?)?;
    m.add_class::<GraphConfig>()?;
    m.add_class::<Graph>()?;
    m.add_class::<RelatedSymbol>()?;
    m.add_class::<DefRefPair>()?;
    m.add_class::<RelatedFileContext>()?;
    m.add_class::<FileMetadata>()?;
    m.add_class::<Symbol>()?;
    Ok(())
}

define_stub_info_gatherer!(stub_info);
