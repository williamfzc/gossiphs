/*
File: api.rs
Functionality: High-level API structures and implementation for graph queries.
Role: Defines the core data structures for file relationships and metadata, and implements methods on the Graph struct for easy data retrieval.
*/
use crate::graph::{Graph, RelatedSymbol};
use crate::symbol::{DefRefPair, RangeWrapper, Symbol, SymbolKind};
use indicatif::ProgressBar;
use pyo3::{pyclass, pymethods};
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone)]
#[pyclass]
pub struct RelatedFileContext {
    #[pyo3(get)]
    pub name: String,

    #[pyo3(get)]
    pub score: usize,

    #[pyo3(get)]
    pub defs: usize,

    #[pyo3(get)]
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
            .map(|each| each.path.as_ref().clone())
            .collect()
    }

    /// All files which pointed to this file or pointed by this file
    pub fn related_files(&self, file_name: String) -> Vec<RelatedFileContext> {
        let file_name_arc = Arc::new(file_name);
        if !self.symbol_graph.file_mapping.contains_key(&file_name_arc) {
            return Vec::new();
        }

        let mut file_counter = HashMap::new();
        let mut file_ref_mapping: HashMap<Arc<String>, Vec<RelatedSymbol>> = HashMap::new();

        // 1. Other files -> this file (Incoming)
        let definitions_in_file = self.symbol_graph.list_definitions(&file_name_arc);
        let definition_count = definitions_in_file.len();

        definitions_in_file.iter().for_each(|def| {
            self.symbol_graph
                .list_references_by_definition(&def.id())
                .iter()
                .for_each(|(each_ref, weight)| {
                    let real_weight = if definition_count > 0 { std::cmp::max(weight / definition_count, 1) } else { *weight };

                    file_counter.entry(each_ref.file.clone()).and_modify(|w| *w += real_weight).or_insert(real_weight);

                    file_ref_mapping
                        .entry(each_ref.file.clone())
                        .or_insert_with(Vec::new)
                        .push(RelatedSymbol {
                            symbol: each_ref.clone(),
                            weight: real_weight,
                        });
                });
        });

        // 2. This file -> other files (Outgoing)
        let references_in_file = self.symbol_graph.list_references(&file_name_arc);
        references_in_file.iter().for_each(|ref_symbol| {
            self.symbol_graph
                .list_definitions_by_reference(&ref_symbol.id())
                .iter()
                .for_each(|(each_def, weight)| {
                    file_counter.entry(each_def.file.clone()).and_modify(|w| *w += weight).or_insert(*weight);

                    file_ref_mapping
                        .entry(each_def.file.clone())
                        .or_insert_with(Vec::new)
                        .push(RelatedSymbol {
                            symbol: each_def.clone(),
                            weight: *weight,
                        });
                });
        });

        // remove itself
        file_counter.remove(&file_name_arc);

        let mut contexts = file_counter
            .iter()
            .map(|(k, v)| {
                let related_symbols = file_ref_mapping[k].clone();
                return RelatedFileContext {
                    name: k.as_ref().clone(),
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
        let file_name_arc = Arc::new(file_name.clone());
        let symbols = self
            .symbol_graph
            .list_symbols(&file_name_arc)
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
        let src_file_arc = Arc::new(src_file);
        let dst_file_arc = Arc::new(dst_file);
        if !self.symbol_graph.file_mapping.contains_key(&src_file_arc) || !self.symbol_graph.file_mapping.contains_key(&dst_file_arc) {
            return Vec::new();
        }
        self.symbol_graph.pairs_between_files(&src_file_arc, &dst_file_arc)
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
                            if let Some(existing) = symbol_map.get(&symbol_id) {
                                return existing.id;
                            } else {
                                symbol_map.insert(
                                    symbol_id,
                                    SymbolNode {
                                        id: cur_id,
                                        kind: LineKind::SymbolNode,
                                        name: s.symbol.name.as_ref().clone(),
                                        range: s.symbol.range.clone(),
                                    },
                                );
                                cur_id += 1;
                                return cur_id - 1;
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

#[cfg(test)]
mod tests {
    use crate::graph::Graph;
    use crate::symbol::Symbol;
    use tree_sitter::Range;
    use std::sync::Arc;

    #[test]
    fn test_related_files_logic() {
        let mut g = Graph::empty();
        let file_a = String::from("a.rs");
        let file_b = String::from("b.rs");
        let file_c = String::from("c.rs");

        // mock ranges
        let range1 = Range {
            start_byte: 0,
            end_byte: 1,
            start_point: tree_sitter::Point { row: 0, column: 0 },
            end_point: tree_sitter::Point { row: 0, column: 1 },
        };
        let range2 = Range {
            start_byte: 10,
            end_byte: 11,
            start_point: tree_sitter::Point { row: 1, column: 0 },
            end_point: tree_sitter::Point { row: 1, column: 1 },
        };

        // File A defines "foo" and "bar"
        let def_foo = Symbol::new_def(Arc::new(file_a.clone()), Arc::new(String::from("foo")), range1);
        let def_bar = Symbol::new_def(Arc::new(file_a.clone()), Arc::new(String::from("bar")), range2);

        // File B references "foo" (weight 10)
        let ref_foo_b = Symbol::new_ref(Arc::new(file_b.clone()), Arc::new(String::from("foo")), range1);
        // File C references "foo" (weight 5) and "bar" (weight 5)
        let ref_foo_c = Symbol::new_ref(Arc::new(file_c.clone()), Arc::new(String::from("foo")), range1);
        let ref_bar_c = Symbol::new_ref(Arc::new(file_c.clone()), Arc::new(String::from("bar")), range2);

        g.symbol_graph.add_file(Arc::new(file_a.clone()));
        g.symbol_graph.add_file(Arc::new(file_b.clone()));
        g.symbol_graph.add_file(Arc::new(file_c.clone()));

        for s in &[&def_foo, &def_bar, &ref_foo_b, &ref_foo_c, &ref_bar_c] {
            g.symbol_graph.add_symbol((*s).clone());
            g.symbol_graph.link_file_to_symbol(&(*s).file, *s);
        }

        // Link B -> A (foo)
        g.symbol_graph.link_symbol_to_symbol(&ref_foo_b, &def_foo);
        g.symbol_graph.enhance_symbol_to_symbol(&ref_foo_b.id(), &def_foo.id(), 10);

        // Link C -> A (foo)
        g.symbol_graph.link_symbol_to_symbol(&ref_foo_c, &def_foo);
        g.symbol_graph.enhance_symbol_to_symbol(&ref_foo_c.id(), &def_foo.id(), 5);

        // Link C -> A (bar)
        g.symbol_graph.link_symbol_to_symbol(&ref_bar_c, &def_bar);
        g.symbol_graph.enhance_symbol_to_symbol(&ref_bar_c.id(), &def_bar.id(), 5);

        // Test Incoming: When we look for files related to file_a
        let related_a = g.related_files(file_a.clone());
        assert_eq!(related_a.len(), 2);
        
        // B: 10 / 2 = 5
        // C: (5/2) + (5/2) = 2 + 2 = 4
        assert_eq!(related_a[0].name, file_b);
        assert_eq!(related_a[0].score, 5);
        assert_eq!(related_a[1].name, file_c);
        assert_eq!(related_a[1].score, 4);

        // Test Outgoing: When we look for files related to file_b
        // File B references File A (foo) with weight 10
        let related_b = g.related_files(file_b.clone());
        assert_eq!(related_b.len(), 1);
        assert_eq!(related_b[0].name, file_a);
        assert_eq!(related_b[0].score, 10);
    }
}
