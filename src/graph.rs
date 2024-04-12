use crate::extractor::Extractor;
use crate::symbol::{Symbol, SymbolKind};
use cupido::collector::config::Collect;
use cupido::collector::config::{get_collector, Config};
use cupido::relation::graph::RelationGraph;
use petgraph::graph::{NodeIndex, UnGraph};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};

pub struct FileContext {
    pub path: String,
    pub symbols: Vec<Symbol>,
}

impl FileContext {
    pub fn global_symbol_id(&self, symbol: &Symbol) -> String {
        return format!("{}{}", self.path, symbol.id());
    }
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
                    let symbols = Extractor::Rust.extract(file_content);
                    let file_context = FileContext {
                        // use the relative path as key
                        path: each_file.clone(),
                        symbols,
                    };
                    file_contexts.push(file_context);
                }
                "ts" | "tsx" => {
                    let symbols = Extractor::TypeScript.extract(file_content);
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
                let global_id = &file_context.global_symbol_id(&symbol);
                symbol_graph.add_symbol(global_id, symbol.clone());
                symbol_graph.link_file_to_symbol(&file_context.path, global_id);
            }
        }

        // 2
        for file_context in &final_file_contexts {
            for symbol in &file_context.symbols {
                if symbol.kind == SymbolKind::DEF {
                    continue;
                }
                let defs = global_symbol_table.get(&symbol.name).unwrap();
                let global_symbol_id = &file_context.global_symbol_id(symbol);
                for def in defs {
                    let global_id = &file_context.global_symbol_id(def);
                    symbol_graph.link_symbol_to_symbol(global_symbol_id, global_id);
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

pub enum NodeType {
    File,
    Symbol(Option<SymbolData>),
}

impl NodeType {
    pub fn get_symbol_data(&self) -> Option<&SymbolData> {
        if let NodeType::Symbol(Some(ref data)) = self {
            return Some(data);
        }
        None
    }
}
#[derive(Clone)]
pub struct SymbolData {
    symbol: Symbol,
}

pub struct NodeData {
    node_type: NodeType,
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
            node_type: NodeType::File,
        });
        self.file_mapping.entry(id).or_insert(index);
    }

    pub fn add_symbol(&mut self, id: &String, symbol: Symbol) {
        let id = Arc::new(id.clone());
        if self.symbol_mapping.contains_key(&id) {
            return;
        }

        let index = self.g.add_node(NodeData {
            node_type: NodeType::Symbol(Some(SymbolData { symbol })),
        });
        self.symbol_mapping.entry(id).or_insert(index);
    }

    pub fn link_file_to_symbol(&mut self, name: &String, symbol_id: &String) {
        if let (Some(file_index), Some(symbol_index)) = (
            self.file_mapping.get(name),
            self.symbol_mapping.get(symbol_id),
        ) {
            self.g.add_edge(*file_index, *symbol_index, 0);
        }
    }

    pub fn link_symbol_to_symbol(&mut self, a: &String, b: &String) {
        if let (Some(a_index), Some(b_index)) =
            (self.symbol_mapping.get(a), self.symbol_mapping.get(b))
        {
            self.g.add_edge(*a_index, *b_index, 0);
        }
    }
}

// Read API
impl SymbolGraph {
    pub fn list_symbols(&self, file_name: &String) -> Vec<Symbol> {
        if !self.file_mapping.contains_key(file_name) {
            return Vec::new();
        }

        let file_data = self.file_mapping.get(file_name).unwrap();
        let ids = self
            .g
            .neighbors(*file_data)
            .map(|each| {
                return self.g[each]
                    .node_type
                    .get_symbol_data()
                    .unwrap()
                    .symbol
                    .clone();
            })
            .collect();
        return ids;
    }

    pub fn list_definitions(&self, file_name: &String) -> Vec<Symbol> {
        self.list_symbols(file_name)
            .into_iter()
            .filter(|symbol| symbol.kind == SymbolKind::DEF)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::{Graph, GraphConfig};
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

        g.symbol_graph
            .list_symbols(&String::from(
                "languages/tree-sitter-stack-graphs-typescript/build.rs",
            ))
            .iter()
            .for_each(|each| {
                info!(
                    "{:?} {}: {}:{}",
                    each.kind, each.name, each.range.start_point.row, each.range.start_point.column
                )
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
            .list_symbols(&String::from(
                "languages/tree-sitter-stack-graphs-typescript/build.rs",
            ))
            .iter()
            .for_each(|each| {
                info!(
                    "{:?} {}: {}:{}",
                    each.kind, each.name, each.range.start_point.row, each.range.start_point.column
                )
            });
    }
}
