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
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[pyclass]
pub struct Symbol {
    #[pyo3(get)]
    pub file: String,

    #[pyo3(get)]
    pub name: String,

    #[pyo3(get)]
    pub range: RangeWrapper,

    pub kind: SymbolKind,
}

#[pymethods]
impl Symbol {
    fn is_def(&self) -> bool {
        self.kind == SymbolKind::DEF
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
    pub fn new_def(file: String, name: String, range: Range) -> Symbol {
        Symbol {
            file,
            name,
            kind: SymbolKind::DEF,
            range: RangeWrapper::from(range),
        }
    }

    pub fn new_ref(file: String, name: String, range: Range) -> Symbol {
        Symbol {
            file,
            name,
            kind: SymbolKind::REF,
            range: RangeWrapper::from(range),
        }
    }

    pub fn id(&self) -> String {
        format!("{}{}", self.file, self.range.start_byte)
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

    pub(crate) fn add_file(&mut self, name: &String) {
        let id = Arc::new(name.clone());
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

    pub(crate) fn link_file_to_symbol(&mut self, name: &String, symbol: &Symbol) {
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
            let edge = self.g.find_edge(*a_index, *b_index).unwrap();
            if let Some(weight) = self.g.edge_weight_mut(edge) {
                *weight += ratio;
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

    pub fn list_symbols(&self, file_name: &String) -> Vec<Symbol> {
        if !self.file_mapping.contains_key(file_name) {
            return Vec::new();
        }

        let file_index = self.file_mapping.get(file_name).unwrap();
        self.neighbor_symbols(*file_index)
            .keys()
            .map(|each| each.clone())
            .collect()
    }

    pub fn list_definitions(&self, file_name: &String) -> Vec<Symbol> {
        self.list_symbols(file_name)
            .into_iter()
            .filter(|symbol| symbol.kind == SymbolKind::DEF)
            .collect()
    }

    pub fn list_references(&self, file_name: &String) -> Vec<Symbol> {
        self.list_symbols(file_name)
            .into_iter()
            .filter(|symbol| symbol.kind == SymbolKind::REF)
            .collect()
    }

    pub fn list_references_by_definition(&self, symbol_id: &String) -> HashMap<Symbol, usize> {
        if !self.symbol_mapping.contains_key(symbol_id) {
            return HashMap::new();
        }

        let def_index = self.symbol_mapping.get(symbol_id).unwrap();
        self.neighbor_symbols(*def_index)
    }

    pub fn list_definitions_by_reference(&self, symbol_id: &String) -> HashMap<Symbol, usize> {
        // there are more than one possible definitions
        if !self.symbol_mapping.contains_key(symbol_id) {
            return HashMap::new();
        }

        let ref_index = self.symbol_mapping.get(symbol_id).unwrap();
        self.neighbor_symbols(*ref_index)
    }

    pub fn pairs_between_files(&self, src_file: &String, dst_file: &String) -> Vec<DefRefPair> {
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

#[pyclass]
pub struct DefRefPair {
    #[pyo3(get)]
    pub src_symbol: Symbol,
    #[pyo3(get)]
    pub dst_symbol: Symbol,
}
