/*
File: stub_gen.rs
Functionality: Python stub generation utility.
Role: A helper binary that generates Python type hint stubs for the PyO3-based library, improving development experience for Python users.
*/
use pyo3_stub_gen::Result;

fn main() -> Result<()> {
    // `stub_info` is a function defined by `define_stub_info_gatherer!` macro.
    let stub = gossiphs::stub_info()?;
    stub.generate()?;
    Ok(())
}