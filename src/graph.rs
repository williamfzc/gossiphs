use crate::extractor::Extractor;
use crate::symbol::{Symbol, SymbolKind};
use cupido::collector::config::Collect;
use cupido::collector::config::{get_collector, Config};
use cupido::relation::graph::RelationGraph;
use petgraph::graph::{NodeIndex, UnGraph};
use petgraph::visit::EdgeRef;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::ops::{Add, AddAssign};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};

pub struct FileContext {
    pub path: String,
    pub symbols: Vec<Symbol>,
}

pub struct Graph {
    pub file_contexts: Vec<FileContext>,
    pub relation_graph: RelationGraph,
    pub symbol_graph: SymbolGraph,
}

impl Graph {
    fn extract_file_contexts(root: &String, files: Vec<String>) -> Vec<FileContext> {
        let mut file_contexts: Vec<FileContext> = Vec::new();

        for each_file in &files {
            let file_path = &Path::new(&root)
                .join(each_file)
                .to_string_lossy()
                .into_owned();
            if fs::metadata(file_path).is_err() {
                continue;
            }

            let file_extension = match file_path.split('.').last() {
                Some(ext) => ext.to_lowercase(),
                None => {
                    debug!("File {} has no extension, skipping...", file_path);
                    continue;
                }
            };

            let file_content = &fs::read_to_string(file_path).unwrap_or_default();
            if file_content.is_empty() {
                continue;
            }
            match file_extension.as_str() {
                "rs" => {
                    let symbols = Extractor::Rust.extract(each_file, file_content);
                    let file_context = FileContext {
                        // use the relative path as key
                        path: each_file.clone(),
                        symbols,
                    };
                    file_contexts.push(file_context);
                }
                "ts" | "tsx" => {
                    let symbols = Extractor::TypeScript.extract(each_file, file_content);
                    let file_context = FileContext {
                        // use the relative path as key
                        path: each_file.clone(),
                        symbols,
                    };
                    file_contexts.push(file_context);
                }
                _ => {}
            }
        }
        // extract ok
        info!("symbol extract finished, files: {}", file_contexts.len());
        return file_contexts;
    }

    fn build_global_symbol_table(file_contexts: &[FileContext]) -> HashMap<String, Vec<Symbol>> {
        let mut global_symbol_table: HashMap<String, Vec<Symbol>> = HashMap::new();

        file_contexts
            .iter()
            .flat_map(|file_context| file_context.symbols.iter())
            .filter(|symbol| symbol.kind == SymbolKind::DEF)
            .for_each(|symbol| {
                global_symbol_table
                    .entry(symbol.name.clone())
                    .or_insert_with(Vec::new)
                    .push(symbol.clone());
            });

        global_symbol_table
    }

    fn filter_pointless_symbols(
        file_contexts: &Vec<FileContext>,
        global_symbol_table: &HashMap<String, Vec<Symbol>>,
    ) -> Vec<FileContext> {
        let mut filtered_file_contexts = Vec::new();
        for file_context in file_contexts {
            let filtered_symbols = file_context
                .symbols
                .iter()
                .filter(|symbol| global_symbol_table.contains_key(&symbol.name))
                .map(|symbol| symbol.clone())
                .collect();

            filtered_file_contexts.push(FileContext {
                path: file_context.path.clone(),
                symbols: filtered_symbols,
            });
        }
        filtered_file_contexts
    }

    pub fn from(conf: GraphConfig) -> Graph {
        // 1. call cupido
        // 2. extract symbols
        // 3. building def and ref relations
        let relation_graph = create_cupido_graph(&conf.project_path);
        let size = relation_graph.size();
        debug!("relation graph size: {:?}", size);

        let files = relation_graph.files();
        let file_contexts = Self::extract_file_contexts(&conf.project_path, files);

        // filter pointless REF
        let mut global_symbol_table: HashMap<String, Vec<Symbol>> =
            Self::build_global_symbol_table(&file_contexts);
        let final_file_contexts =
            Self::filter_pointless_symbols(&file_contexts, &global_symbol_table);

        for file_context in &final_file_contexts {
            // and collect all the definitions
            // k is name, v is location
            file_context
                .symbols
                .iter()
                .filter(|symbol| symbol.kind == SymbolKind::DEF)
                .for_each(|symbol| {
                    if let Some(v) = global_symbol_table.get_mut(&symbol.name) {
                        v.push(symbol.clone());
                    }
                });
        }

        // building graph
        // 1. file - symbols
        // 2. connect defs and refs
        // 3. priority recalculation
        let mut symbol_graph = SymbolGraph::new();
        for file_context in &final_file_contexts {
            symbol_graph.add_file(&file_context.path);
            for symbol in &file_context.symbols {
                symbol_graph.add_symbol(symbol.clone());
                symbol_graph.link_file_to_symbol(&file_context.path, symbol);
            }
        }

        // 2
        for file_context in &final_file_contexts {
            for symbol in &file_context.symbols {
                if symbol.kind != SymbolKind::REF {
                    continue;
                }
                // find all the related definitions, and connect to them
                let defs = global_symbol_table.get(&symbol.name).unwrap();
                for def in defs {
                    symbol_graph.link_symbol_to_symbol(def, symbol);
                }
            }
        }

        // 3
        // commit cache
        let mut cache: HashMap<String, HashSet<String>> = HashMap::new();
        let mut related_commits = |f: String| -> HashSet<String> {
            if let Some(ref_commits) = cache.get(&f) {
                return ref_commits.clone();
            } else {
                let file_commits: HashSet<String> = relation_graph
                    .file_related_commits(&f)
                    .unwrap()
                    .into_iter()
                    .collect();

                cache.insert(f.clone(), file_commits.clone());
                return file_commits;
            }
        };

        for file_context in &final_file_contexts {
            let def_related_commits = related_commits(file_context.path.clone());
            for symbol in &file_context.symbols {
                if symbol.kind != SymbolKind::REF {
                    continue;
                }
                let defs = global_symbol_table.get(&symbol.name).unwrap();
                for def in defs {
                    let f = def.file.clone();
                    let ref_related_commits = related_commits(f);
                    // calc the diff of two set
                    let intersection: HashSet<String> = ref_related_commits
                        .intersection(&def_related_commits)
                        .cloned()
                        .collect();
                    let ratio = intersection.len();
                    symbol_graph.enhance_symbol_to_symbol(&symbol.id(), &def.id(), ratio);
                }
            }
        }

        info!(
            "symbol graph ready, nodes: {}, edges: {}",
            symbol_graph.symbol_mapping.len(),
            symbol_graph.g.edge_count(),
        );

        return Graph {
            file_contexts,
            relation_graph,
            symbol_graph,
        };
    }
}

fn create_cupido_graph(project_path: &String) -> RelationGraph {
    let mut conf = Config::default();
    conf.repo_path = project_path.parse().unwrap();

    let collector = get_collector();
    let graph = collector.walk(conf);
    return graph;
}

pub struct GraphConfig {
    project_path: String,
}

impl GraphConfig {
    pub fn default() -> GraphConfig {
        return GraphConfig {
            project_path: String::from("."),
        };
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
    _id: Arc<String>,
    node_type: NodeType,
}

impl NodeData {
    pub fn get_symbol(&self) -> Option<Symbol> {
        match &self.node_type {
            NodeType::Symbol(symbol_data) => {
                return Some(symbol_data.symbol.clone());
            }
            _ => None,
        }
    }
}

pub struct SymbolGraph {
    file_mapping: HashMap<Arc<String>, NodeIndex>,
    symbol_mapping: HashMap<Arc<String>, NodeIndex>,
    g: UnGraph<NodeData, usize>,
}

impl SymbolGraph {
    pub fn new() -> SymbolGraph {
        return SymbolGraph {
            file_mapping: HashMap::new(),
            symbol_mapping: HashMap::new(),
            g: UnGraph::<NodeData, usize>::new_undirected(),
        };
    }

    pub fn add_file(&mut self, name: &String) {
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

    pub fn add_symbol(&mut self, symbol: Symbol) {
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

    pub fn link_file_to_symbol(&mut self, name: &String, symbol: &Symbol) {
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

    pub fn link_symbol_to_symbol(&mut self, a: &Symbol, b: &Symbol) {
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

    pub fn enhance_symbol_to_symbol(&mut self, a: &String, b: &String, ratio: usize) {
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
        return self
            .g
            .edges(idx)
            .filter_map(|edge| {
                let target_idx = edge.target();
                let weight = *edge.weight();
                return if let (Some(symbol)) = self.g[target_idx].get_symbol() {
                    Some((symbol.clone(), weight))
                } else {
                    // not a symbol node
                    None
                };
            })
            .collect();
    }

    pub fn list_symbols(&self, file_name: &String) -> HashMap<Symbol, usize> {
        if !self.file_mapping.contains_key(file_name) {
            return HashMap::new();
        }

        let file_index = self.file_mapping.get(file_name).unwrap();
        return self.neighbor_symbols(*file_index);
    }

    pub fn list_definitions(&self, file_name: &String) -> HashMap<Symbol, usize> {
        return self
            .list_symbols(file_name)
            .into_iter()
            .filter(|(symbol, _)| symbol.kind == SymbolKind::DEF)
            .collect();
    }

    pub fn list_references_by_definition(&self, symbol_id: &String) -> HashMap<Symbol, usize> {
        if !self.symbol_mapping.contains_key(symbol_id) {
            return HashMap::new();
        }

        let def_index = self.symbol_mapping.get(symbol_id).unwrap();
        return self.neighbor_symbols(*def_index);
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::{Graph, GraphConfig};
    use crate::symbol::SymbolKind;
    use petgraph::visit::{EdgeRef, IntoEdgeReferences};
    use tracing::{debug, info};

    #[test]
    fn rust_graph() {
        tracing_subscriber::fmt::init();
        let mut config = GraphConfig::default();
        config.project_path = String::from("../stack-graphs");
        let g = Graph::from(config);
        g.file_contexts.iter().for_each(|context| {
            debug!("{}: {:?}", context.path, context.symbols);
        });

        // "stack-graphs/src/stitching.rs2505"
        g.symbol_graph.g.edge_references().for_each(|each| {
            if *each.weight() > 0 {
                debug!(
                    "{:?} {:?} -- {:?} {:?}, {}",
                    g.symbol_graph.g[each.source()]._id,
                    g.symbol_graph.g[each.source()].get_symbol().unwrap().kind,
                    g.symbol_graph.g[each.target()]._id,
                    g.symbol_graph.g[each.target()].get_symbol().unwrap().kind,
                    each.weight()
                )
            }
        });

        g.symbol_graph
            .list_definitions(&String::from(
                "tree-sitter-stack-graphs/src/cli/util/reporter.rs",
            ))
            .iter()
            .for_each(|(each, _)| {
                g.symbol_graph
                    .list_references_by_definition(&each.id())
                    .iter()
                    .for_each(|(each_ref, weight)| {
                        debug!("{} ref in {}, weight {}", each.file, each_ref.file, weight);
                    });
            });
    }

    #[test]
    fn ts_graph() {
        tracing_subscriber::fmt::init();
        let mut config = GraphConfig::default();
        config.project_path = String::from("../lsif-node");
        let g = Graph::from(config);
        g.file_contexts.iter().for_each(|context| {
            debug!("{}: {:?}", context.path, context.symbols);
        });

        g.symbol_graph
            .list_symbols(&String::from("lsif/src/main.ts"))
            .iter()
            .for_each(|(each, weight)| {
                debug!(
                    "{weight} {:?} {}: {}:{}",
                    each.kind, each.name, each.range.start_point.row, each.range.start_point.column
                )
            });
    }
}
