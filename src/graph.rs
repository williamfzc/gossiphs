use crate::extractor::Extractor;
use crate::symbol::{Symbol, SymbolGraph, SymbolKind};
use cupido::collector::config::Collect;
use cupido::collector::config::{get_collector, Config};
use cupido::relation::graph::RelationGraph;
use indicatif::ProgressBar;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
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

        let filtered_files: Vec<(String, String)> = files
            .iter()
            .filter_map(|each_file| {
                let file_path = &Path::new(&root)
                    .join(each_file)
                    .to_string_lossy()
                    .into_owned();
                if fs::metadata(file_path).is_err() {
                    return None;
                }

                return Some((each_file.clone(), file_path.clone()));
            })
            .collect();

        let pb = ProgressBar::new(filtered_files.len() as u64);
        for (each_file, file_path) in &filtered_files {
            pb.inc(1);
            let file_extension = match each_file.split('.').last() {
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
                "go" => {
                    let symbols = Extractor::Go.extract(each_file, file_content);
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
        pb.finish_and_clear();
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
        edge_limit: usize,
    ) -> Vec<FileContext> {
        let mut filtered_file_contexts = Vec::new();
        for file_context in file_contexts {
            let filtered_symbols = file_context
                .symbols
                .iter()
                .filter(|symbol| global_symbol_table.contains_key(&symbol.name))
                .filter(|symbol| global_symbol_table[&symbol.name].len() <= edge_limit)
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
        info!("relation graph ready, size: {:?}", size);

        let files = relation_graph.files();
        let file_contexts = Self::extract_file_contexts(&conf.project_path, files);
        info!("symbol extract finished, files: {}", file_contexts.len());

        // filter pointless REF
        let mut global_symbol_table: HashMap<String, Vec<Symbol>> =
            Self::build_global_symbol_table(&file_contexts);
        let final_file_contexts =
            Self::filter_pointless_symbols(&file_contexts, &global_symbol_table, conf.edge_limit);

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
        info!("start building symbol graph ...");
        let pb = ProgressBar::new(final_file_contexts.len() as u64);
        let mut symbol_graph = SymbolGraph::new();
        for file_context in &final_file_contexts {
            pb.inc(1);
            symbol_graph.add_file(&file_context.path);
            for symbol in &file_context.symbols {
                symbol_graph.add_symbol(symbol.clone());
                symbol_graph.link_file_to_symbol(&file_context.path, symbol);
            }
        }
        pb.finish_and_clear();
        pb.reset();

        // 2
        for file_context in &final_file_contexts {
            pb.inc(1);
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
        pb.finish_and_clear();
        pb.reset();

        // 3
        // commit cache
        let mut cache: HashMap<String, HashSet<String>> = HashMap::new();
        let mut related_commits = |f: String| -> HashSet<String> {
            return if let Some(ref_commits) = cache.get(&f) {
                ref_commits.clone()
            } else {
                let file_commits: HashSet<String> = relation_graph
                    .file_related_commits(&f)
                    .unwrap()
                    .into_iter()
                    .collect();

                cache.insert(f.clone(), file_commits.clone());
                file_commits
            };
        };

        for file_context in &final_file_contexts {
            pb.inc(1);
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
        pb.finish_and_clear();

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

// Read API
impl Graph {
    pub fn files(&self) -> HashSet<String> {
        return self
            .file_contexts
            .iter()
            .map(|each| each.path.clone())
            .collect();
    }

    pub fn file_exists(&self, file_name: &String) -> bool {
        return self.files().contains(file_name);
    }

    pub fn related_files(&self, file_name: &String) -> Vec<RelatedFileContext> {
        if !self.file_exists(file_name) {
            return Vec::new();
        }

        // find all the defs in this file
        // and tracking all the references and theirs
        let mut file_counter = HashMap::new();
        self.symbol_graph
            .list_definitions(file_name)
            .iter()
            .for_each(|(def, _)| {
                self.symbol_graph
                    .list_references_by_definition(&def.id())
                    .iter()
                    .for_each(|(each_ref, weight)| {
                        file_counter.entry(each_ref.file.clone()).or_insert(0);
                        file_counter
                            .entry(each_ref.file.clone())
                            .and_modify(|w| *w += *weight)
                            .or_insert(*weight);
                    });
            });
        self.symbol_graph
            .list_references(file_name)
            .iter()
            .for_each(|(each_ref, _)| {
                let defs = self
                    .symbol_graph
                    .list_definitions_by_reference(&each_ref.id());

                defs.iter().for_each(|(each_def, weight)| {
                    file_counter.entry(each_def.file.clone()).or_insert(0);
                    file_counter
                        .entry(each_def.file.clone())
                        .and_modify(|w| *w += *weight)
                        .or_insert(*weight);
                })
            });
        file_counter.remove(file_name);

        let mut contexts = file_counter
            .iter()
            .map(|(k, v)| {
                return RelatedFileContext {
                    name: k.clone(),
                    score: *v,
                    def_units: self.symbol_graph.list_definitions(k).len(),
                    ref_units: self.symbol_graph.list_references(k).len(),
                };
            })
            .collect::<Vec<_>>();
        contexts.sort_by_key(|context| Reverse(context.score));
        return contexts;
    }
}

#[derive(Serialize, Deserialize)]
pub struct RelatedFileContext {
    pub name: String,
    pub score: usize,
    pub def_units: usize,
    pub ref_units: usize,
}

fn create_cupido_graph(project_path: &String) -> RelationGraph {
    let mut conf = Config::default();
    conf.repo_path = project_path.parse().unwrap();

    let collector = get_collector();
    let graph = collector.walk(conf);
    return graph;
}

pub struct GraphConfig {
    pub project_path: String,
    pub edge_limit: usize,
}

impl GraphConfig {
    pub fn default() -> GraphConfig {
        return GraphConfig {
            project_path: String::from("."),
            edge_limit: 128,
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::{Graph, GraphConfig};
    use petgraph::visit::EdgeRef;
    use tracing::{debug, info};

    #[test]
    fn symbol_graph_rust() {
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
    fn symbol_graph_ts() {
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

    #[test]
    fn graph_api() {
        tracing_subscriber::fmt::init();
        let mut config = GraphConfig::default();
        config.project_path = String::from("../stack-graphs");
        let g = Graph::from(config);
        let files = g.related_files(&String::from(
            "tree-sitter-stack-graphs/src/cli/util/reporter.rs",
        ));
        files.iter().for_each(|item| {
            info!("{}: {}", item.name, item.score);
        });
    }
}
