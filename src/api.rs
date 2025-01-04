use crate::graph::{Graph, RelatedSymbol};
use crate::symbol::{DefRefPair, RangeWrapper, Symbol, SymbolKind};
use indicatif::ProgressBar;
use pyo3::{pyclass, pymethods};
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

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
    pub path: String,

    #[pyo3(get)]
    pub commits: Vec<String>,

    #[pyo3(get)]
    pub symbols: Vec<Symbol>,

    #[pyo3(get)]
    pub issues: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[pyclass]
pub enum LineKind {
    FileNode,
    FileRelation,
    SymbolNode,
}

#[derive(Serialize, Deserialize, Clone)]
#[pyclass]
pub struct FileNode {
    #[pyo3(get)]
    id: usize,

    #[pyo3(get)]
    kind: LineKind,

    #[pyo3(get)]
    name: String,

    #[pyo3(get)]
    issues: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[pyclass]
pub struct FileRelation {
    #[pyo3(get)]
    id: usize,

    #[pyo3(get)]
    kind: LineKind,

    #[pyo3(get)]
    src: usize,

    #[pyo3(get)]
    dst: usize,

    #[pyo3(get)]
    symbols: Vec<usize>,
}

#[derive(Serialize, Deserialize, Clone)]
#[pyclass]
pub struct SymbolNode {
    #[pyo3(get)]
    id: usize,

    #[pyo3(get)]
    kind: LineKind,

    #[pyo3(get)]
    name: String,

    #[pyo3(get)]
    range: RangeWrapper,
}

#[derive(Serialize, Deserialize, Clone)]
#[pyclass]
pub struct RelationList {
    #[pyo3(get)]
    pub file_nodes: Vec<FileNode>,

    #[pyo3(get)]
    pub file_relations: Vec<FileRelation>,

    #[pyo3(get)]
    pub symbol_nodes: Vec<SymbolNode>,
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

        definitions_in_file.iter().for_each(|def| {
            self.symbol_graph
                .list_references_by_definition(&def.id())
                .into_iter()
                .map(|s| s.0.file)
                .for_each(|f| {
                    file_ref_mapping
                        .entry(f.clone())
                        .and_modify(|v| {
                            v.push(RelatedSymbol {
                                symbol: def.clone(),
                                weight: 0,
                            })
                        })
                        .or_insert(vec![RelatedSymbol {
                            symbol: def.clone(),
                            weight: 0,
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
            _ => HashMap::new(),
        }
    }

    pub fn file_metadata(&self, file_name: String) -> FileMetadata {
        let symbols = self
            .symbol_graph
            .list_symbols(&file_name)
            .iter()
            .cloned()
            .collect();

        let commit_sha_list = self
            ._relation_graph
            .file_related_commits(&file_name)
            .unwrap_or_default();

        let issue_list = self
            ._relation_graph
            .file_related_issues(&file_name)
            .unwrap_or_default();

        FileMetadata {
            path: file_name,
            commits: commit_sha_list,
            issues: issue_list,
            symbols,
        }
    }

    pub fn pairs_between_files(&self, src_file: String, dst_file: String) -> Vec<DefRefPair> {
        if !self.files().contains(&src_file) || !self.files().contains(&dst_file) {
            return Vec::new();
        }
        self.symbol_graph.pairs_between_files(&src_file, &dst_file)
    }

    pub fn list_file_issues(&self, file_name: String) -> Vec<String> {
        let result = self._relation_graph.file_related_issues(&file_name);
        result.unwrap_or_default()
    }

    pub fn list_file_commits(&self, file_name: String) -> Vec<String> {
        let result = self._relation_graph.file_related_commits(&file_name);
        result.unwrap_or_default()
    }

    pub fn list_all_relations(&self) -> RelationList {
        // https://github.com/williamfzc/gossiphs/issues/38
        // node: file, symbol
        // edge: file relation
        let mut files: Vec<String> = self.files().into_iter().collect();
        files.sort();
        let file_id_map: HashMap<&String, usize> = files
            .iter()
            .enumerate()
            .map(|(i, file)| (file, i))
            .collect();

        let pb = ProgressBar::new(files.len() as u64);
        let results: HashMap<&String, Vec<RelatedFileContext>> = files
            .par_iter()
            .map(|file| {
                pb.inc(1);
                let related_files: Vec<RelatedFileContext> =
                    self.related_files(file.clone()).into_iter().collect();
                return (file, related_files);
            })
            .collect();
        pb.finish_and_clear();

        let mut file_nodes: Vec<FileNode> = Vec::new();
        let mut file_relations: Vec<FileRelation> = Vec::new();
        for (file, id) in &file_id_map {
            file_nodes.push(FileNode {
                id: id.clone(),
                kind: LineKind::FileNode,
                name: file.to_string(),
                issues: self.list_file_issues(file.to_string()),
            });
        }

        let mut symbol_map: HashMap<String, SymbolNode> = HashMap::new();
        let mut cur_id = file_nodes.len();
        for (file, related_files) in &results {
            let src_id = file_id_map[file];
            for related_file in related_files {
                if let Some(&dst_id) = file_id_map.get(&related_file.name) {
                    let symbols: Vec<usize> = related_file
                        .related_symbols
                        .iter()
                        .filter(|s| s.symbol.kind == SymbolKind::DEF)
                        .map(|s| {
                            let symbol_id = s.symbol.id();
                            if !symbol_map.contains_key(&symbol_id) {
                                symbol_map.insert(
                                    symbol_id,
                                    SymbolNode {
                                        id: cur_id,
                                        kind: LineKind::SymbolNode,
                                        name: s.symbol.name.clone(),
                                        range: s.symbol.range.clone(),
                                    },
                                );
                                cur_id += 1;
                                return cur_id - 1;
                            } else {
                                return symbol_map.get(&symbol_id).unwrap().id;
                            }
                        })
                        .collect::<HashSet<_>>()
                        .into_iter()
                        .collect();
                    file_relations.push(FileRelation {
                        id: cur_id,
                        kind: LineKind::FileRelation,
                        src: src_id,
                        dst: dst_id,
                        symbols,
                    });
                    cur_id += 1;
                }
            }
        }

        RelationList {
            file_nodes,
            file_relations,
            symbol_nodes: symbol_map.values().cloned().collect(),
        }
    }
}
