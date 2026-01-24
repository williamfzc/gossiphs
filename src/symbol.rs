/*
File: symbol.rs
Functionality: Symbol data structures and low-level symbol graph.
Role: Defines the core Symbol and Range types and implements the SymbolGraph using petgraph to track fine-grained code entity relationships.
*/
use petgraph::graph::{NodeIndex, UnGraph};
use petgraph::prelude::EdgeRef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use pyo3::{pyclass, pymethods};
use tree_sitter::Range;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[pyclass]
pub enum SymbolKind {
    DEF,
    REF,
    NAMESPACE,
    IMPORT,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[pyclass]
pub struct Symbol {
    pub file: Arc<String>,

    pub name: Arc<String>,

    #[pyo3(get)]
    pub range: RangeWrapper,

    pub kind: SymbolKind,
}

#[pymethods]
impl Symbol {
    fn is_def(&self) -> bool {
        self.kind == SymbolKind::DEF
    }

    #[getter]
    fn file(&self) -> String {
        self.file.as_ref().clone()
    }

    #[getter]
    fn name(&self) -> String {
        self.name.as_ref().clone()
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[pyclass]
pub struct Point {
    #[pyo3(get)]
    pub row: usize,
    #[pyo3(get)]
    pub column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[pyclass]
pub struct RangeWrapper {
    pub start_byte: usize,
    pub end_byte: usize,
    #[pyo3(get)]
    pub start_point: Point,
    #[pyo3(get)]
    pub end_point: Point,
}

impl RangeWrapper {
    pub fn from(range: Range) -> RangeWrapper {
        RangeWrapper {
            start_byte: range.start_byte,
            end_byte: range.end_byte,
            start_point: Point {
                row: range.start_point.row,
                column: range.start_point.column,
            },
            end_point: Point {
                row: range.end_point.row,
                column: range.end_point.column,
            },
        }
    }
}

impl Symbol {
    pub fn new_def(file: Arc<String>, name: Arc<String>, range: Range) -> Symbol {
        Symbol {
            file,
            name,
            kind: SymbolKind::DEF,
            range: RangeWrapper::from(range),
        }
    }

    pub fn new_ref(file: Arc<String>, name: Arc<String>, range: Range) -> Symbol {
        Symbol {
            file,
            name,
            kind: SymbolKind::REF,
            range: RangeWrapper::from(range),
        }
    }

    pub fn new_namespace(file: Arc<String>, name: Arc<String>, range: Range) -> Symbol {
        Symbol {
            file,
            name,
            kind: SymbolKind::NAMESPACE,
            range: RangeWrapper::from(range),
        }
    }

    pub fn new_import(file: Arc<String>, name: Arc<String>, range: Range) -> Symbol {
        Symbol {
            file,
            name,
            kind: SymbolKind::IMPORT,
            range: RangeWrapper::from(range),
        }
    }

    pub fn id(&self) -> String {
        format!("{}:{}:{:?}:{}", self.file, self.name, self.kind, self.range.start_byte)
    }
}

impl Hash for Symbol {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

#[derive(Clone)]
pub enum NodeType {
    File,
    Symbol(SymbolData),
}

#[derive(Clone)]
pub struct SymbolData {
    symbol: Symbol,
}

#[derive(Clone)]
pub struct NodeData {
    pub(crate) _id: Arc<String>,
    pub(crate) node_type: NodeType,
}

impl NodeData {
    pub fn get_symbol(&self) -> Option<Symbol> {
        match &self.node_type {
            NodeType::Symbol(symbol_data) => Some(symbol_data.symbol.clone()),
            _ => None,
        }
    }
}

pub struct SymbolGraph {
    pub(crate) file_mapping: HashMap<Arc<String>, NodeIndex>,
    pub(crate) symbol_mapping: HashMap<Arc<String>, NodeIndex>,
    pub(crate) g: UnGraph<NodeData, usize>,
}

impl SymbolGraph {
    pub fn new() -> SymbolGraph {
        SymbolGraph {
            file_mapping: HashMap::new(),
            symbol_mapping: HashMap::new(),
            g: UnGraph::<NodeData, usize>::new_undirected(),
        }
    }

    pub(crate) fn add_file(&mut self, id: Arc<String>) {
        if self.file_mapping.contains_key(&id) {
            return;
        }

        let index = self.g.add_node(NodeData {
            _id: id.clone(),
            node_type: NodeType::File,
        });
        self.file_mapping.entry(id).or_insert(index);
    }

    pub(crate) fn add_symbol(&mut self, symbol: Symbol) {
        let id = Arc::new(symbol.id());
        if self.symbol_mapping.contains_key(&id) {
            return;
        }

        let index = self.g.add_node(NodeData {
            _id: id.clone(),
            node_type: NodeType::Symbol(SymbolData { symbol }),
        });
        self.symbol_mapping.entry(id).or_insert(index);
    }

    pub(crate) fn link_file_to_symbol(&mut self, name: &Arc<String>, symbol: &Symbol) {
        if let (Some(file_index), Some(symbol_index)) = (
            self.file_mapping.get(name),
            self.symbol_mapping.get(&symbol.id()),
        ) {
            if let Some(..) = self.g.find_edge(*file_index, *symbol_index) {
                return;
            }
            self.g.add_edge(*file_index, *symbol_index, 0);
        }
    }

    pub(crate) fn link_symbol_to_symbol(&mut self, a: &Symbol, b: &Symbol) {
        if let (Some(a_index), Some(b_index)) = (
            self.symbol_mapping.get(&a.id()),
            self.symbol_mapping.get(&b.id()),
        ) {
            if let Some(..) = self.g.find_edge(*a_index, *b_index) {
                return;
            }
            self.g.add_edge(*a_index, *b_index, 0);
        }
    }

    pub(crate) fn enhance_symbol_to_symbol(&mut self, a: &String, b: &String, ratio: usize) {
        if let (Some(a_index), Some(b_index)) =
            (self.symbol_mapping.get(a), self.symbol_mapping.get(b))
        {
            if let Some(edge) = self.g.find_edge(*a_index, *b_index) {
                if let Some(weight) = self.g.edge_weight_mut(edge) {
                    *weight += ratio;
                }
            }
        }
    }
}

// Read API
impl SymbolGraph {
    fn neighbor_symbols(&self, idx: NodeIndex) -> HashMap<Symbol, usize> {
        self.g
            .edges(idx)
            .filter_map(|edge| {
                let target_idx = edge.target();
                let weight = *edge.weight();
                return if let Some(symbol) = self.g[target_idx].get_symbol() {
                    Some((symbol.clone(), weight))
                } else {
                    // not a symbol node
                    None
                };
            })
            .collect()
    }

    pub fn list_symbols(&self, file_name: &Arc<String>) -> Vec<Symbol> {
        if let Some(file_index) = self.file_mapping.get(file_name) {
            self.neighbor_symbols(*file_index)
                .keys()
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn list_definitions(&self, file_name: &Arc<String>) -> Vec<Symbol> {
        self.list_symbols(file_name)
            .into_iter()
            .filter(|symbol| symbol.kind == SymbolKind::DEF)
            .collect()
    }

    pub fn list_references(&self, file_name: &Arc<String>) -> Vec<Symbol> {
        self.list_symbols(file_name)
            .into_iter()
            .filter(|symbol| symbol.kind == SymbolKind::REF)
            .collect()
    }

    pub fn list_references_by_definition(&self, symbol_id: &String) -> HashMap<Symbol, usize> {
        if let Some(def_index) = self.symbol_mapping.get(symbol_id) {
            self.neighbor_symbols(*def_index)
        } else {
            HashMap::new()
        }
    }

    pub fn list_definitions_by_reference(&self, symbol_id: &String) -> HashMap<Symbol, usize> {
        // there are more than one possible definitions
        if let Some(ref_index) = self.symbol_mapping.get(symbol_id) {
            self.neighbor_symbols(*ref_index)
        } else {
            HashMap::new()
        }
    }

    pub fn pairs_between_files(&self, src_file: &Arc<String>, dst_file: &Arc<String>) -> Vec<DefRefPair> {
        let defs = self.list_definitions(src_file);
        let refs = self.list_references(dst_file);

        let mut pairs = vec![];

        for each_def in &defs {
            let def_index = self.symbol_mapping[&each_def.id()];
            for each_ref in &refs {
                let ref_index = self.symbol_mapping[&each_ref.id()];
                if self.g.contains_edge(def_index, ref_index) {
                    pairs.push(DefRefPair {
                        src_symbol: each_def.clone(),
                        dst_symbol: each_ref.clone(),
                    });
                }
            }
        }
        pairs
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[pyclass]
pub struct DefRefPair {
    #[pyo3(get)]
    pub src_symbol: Symbol,
    #[pyo3(get)]
    pub dst_symbol: Symbol,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Point as TSPoint;

    #[test]
    fn test_symbol_id_and_arc_sharing() {
        let file = Arc::new("src/lib.rs".to_string());
        let name = Arc::new("my_func".to_string());
        let range = Range {
            start_byte: 10,
            end_byte: 20,
            start_point: TSPoint { row: 1, column: 5 },
            end_point: TSPoint { row: 1, column: 15 },
        };

        let sym1 = Symbol::new_def(file.clone(), name.clone(), range);
        let sym2 = Symbol::new_def(file.clone(), name.clone(), range);

        assert_eq!(sym1.id(), sym2.id());
        assert_eq!(sym1, sym2);

        // Verify Arc sharing (same pointer address)
        assert!(Arc::ptr_eq(&sym1.file, &sym2.file));
        assert!(Arc::ptr_eq(&sym1.name, &sym2.name));
    }

    #[test]
    fn test_symbol_kind_diff_ids() {
        let file = Arc::new("src/lib.rs".to_string());
        let name = Arc::new("my_func".to_string());
        let range = Range {
            start_byte: 10,
            end_byte: 20,
            start_point: TSPoint { row: 1, column: 5 },
            end_point: TSPoint { row: 1, column: 15 },
        };

        let sym_def = Symbol::new_def(file.clone(), name.clone(), range);
        let sym_ref = Symbol::new_ref(file.clone(), name.clone(), range);

        assert_ne!(sym_def.id(), sym_ref.id());
    }

    #[test]
    fn test_symbol_graph_basic() {
        let mut graph = SymbolGraph::new();
        let file = Arc::new("test.rs".to_string());
        let sym_name = Arc::new("foo".to_string());
        let range = Range {
            start_byte: 0,
            end_byte: 0,
            start_point: TSPoint { row: 0, column: 0 },
            end_point: TSPoint { row: 0, column: 0 },
        };

        let sym = Symbol::new_def(file.clone(), sym_name.clone(), range);

        graph.add_file(file.clone());
        graph.add_symbol(sym.clone());
        graph.link_file_to_symbol(&file, &sym);

        let symbols = graph.list_symbols(&file);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name.as_ref(), "foo");
    }
}
