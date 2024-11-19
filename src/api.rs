use crate::graph::{Graph, RelatedSymbol};
use crate::symbol::{DefRefPair, Symbol, SymbolKind};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use pyo3::{pyclass, pymethods};

#[derive(Serialize, Deserialize, Clone)]
#[pyclass]
pub struct RelatedFileContext {
    #[pyo3(get)]
    pub name: String,
    pub score: usize,
    pub defs: usize,
    pub refs: usize,

    #[pyo3(get)]
    pub related_symbols: Vec<RelatedSymbol>,
}

#[derive(Serialize, Deserialize)]
#[pyclass]
pub struct FileMetadata {
    #[pyo3(get)]
    pub symbols: Vec<Symbol>,
}

// Read API v1
#[pymethods]
impl Graph {
    pub fn files(&self) -> HashSet<String> {
        self.file_contexts
            .iter()
            .map(|each| each.path.clone())
            .collect()
    }

    /// All files which pointed to this file
    pub fn related_files(&self, file_name: String) -> Vec<RelatedFileContext> {
        if !self.symbol_graph.file_mapping.contains_key(&file_name) {
            return Vec::new();
        }

        // find all the defs in this file
        // and tracking all the references and theirs
        let mut file_counter = HashMap::new();
        let mut file_ref_mapping: HashMap<String, Vec<RelatedSymbol>> = HashMap::new();

        // other files -> this file
        let definitions_in_file = self.symbol_graph.list_definitions(&file_name);
        let definition_count = definitions_in_file.len();

        definitions_in_file.iter().for_each(|def| {
            self.symbol_graph
                .list_references_by_definition(&def.id())
                .iter()
                .for_each(|(each_ref, weight)| {
                    let real_weight = std::cmp::max(weight / definition_count, 1);

                    file_counter.entry(each_ref.file.clone()).or_insert(0);
                    file_counter
                        .entry(each_ref.file.clone())
                        .and_modify(|w| *w += real_weight)
                        .or_insert(real_weight);

                    file_ref_mapping
                        .entry(each_ref.file.clone())
                        .and_modify(|v| {
                            v.push(RelatedSymbol {
                                symbol: each_ref.clone(),
                                weight: real_weight,
                            })
                        })
                        .or_insert(vec![RelatedSymbol {
                            symbol: each_ref.clone(),
                            weight: real_weight,
                        }]);
                });
        });

        // this file -> other files
        // TODO: need it?

        // remove itself
        file_counter.remove(&file_name);

        let mut contexts = file_counter
            .iter()
            .map(|(k, v)| {
                let related_symbols = file_ref_mapping[k].clone();
                return RelatedFileContext {
                    name: k.clone(),
                    score: *v,
                    defs: self.symbol_graph.list_definitions(k).len(),
                    refs: self.symbol_graph.list_references(k).len(),
                    related_symbols,
                };
            })
            .collect::<Vec<_>>();
        contexts.sort_by_key(|context| Reverse(context.score));
        contexts
    }

    pub fn related_symbols(&self, symbol: Symbol) -> HashMap<Symbol, usize> {
        match symbol.kind {
            SymbolKind::DEF => self
                .symbol_graph
                .list_references_by_definition(&symbol.id())
                .into_iter()
                .collect(),
            SymbolKind::REF => self
                .symbol_graph
                .list_definitions_by_reference(&symbol.id())
                .into_iter()
                .collect(),
        }
    }

    pub fn file_metadata(&self, file_name: String) -> FileMetadata {
        let symbols = self
            .symbol_graph
            .list_symbols(&file_name)
            .iter()
            .cloned()
            .collect();
        FileMetadata { symbols }
    }

    pub fn pairs_between_files(&self, src_file: String, dst_file: String) -> Vec<DefRefPair> {
        if !self.files().contains(&src_file) || !self.files().contains(&dst_file) {
            return Vec::new();
        }
        self.symbol_graph.pairs_between_files(&src_file, &dst_file)
    }
}