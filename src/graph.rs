use crate::extractor::Extractor;
use crate::symbol::{Symbol, SymbolGraph, SymbolKind};
use cupido::collector::config::Collect;
use cupido::collector::config::{get_collector, Config};
use cupido::relation::graph::RelationGraph;
use indicatif::ProgressBar;
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::Instant;
use tracing::{debug, info};

pub struct FileContext {
    pub path: String,
    pub symbols: Vec<Symbol>,
}

pub struct Graph {
    pub(crate) file_contexts: Vec<FileContext>,
    pub(crate) _relation_graph: RelationGraph,
    pub(crate) symbol_graph: SymbolGraph,
}

impl Graph {
    fn extract_file_context(file_name: &String, file_path: &String) -> Option<FileContext> {
        let file_extension = match file_name.split('.').last() {
            Some(ext) => ext.to_lowercase(),
            None => {
                debug!("File {} has no extension, skipping...", file_path);
                return None;
            }
        };

        let file_content = &fs::read_to_string(file_path).unwrap_or_default();
        if file_content.is_empty() {
            return None;
        }
        return match file_extension.as_str() {
            "rs" => {
                let symbols = Extractor::Rust.extract(file_name, file_content);
                let file_context = FileContext {
                    // use the relative path as key
                    path: file_name.clone(),
                    symbols,
                };
                Some(file_context)
            }
            "ts" | "tsx" => {
                let symbols = Extractor::TypeScript.extract(file_name, file_content);
                let file_context = FileContext {
                    // use the relative path as key
                    path: file_name.clone(),
                    symbols,
                };
                Some(file_context)
            }
            "go" => {
                let symbols = Extractor::Go.extract(file_name, file_content);
                let file_context = FileContext {
                    // use the relative path as key
                    path: file_name.clone(),
                    symbols,
                };
                Some(file_context)
            }
            "py" => {
                let symbols = Extractor::Python.extract(file_name, file_content);
                let file_context = FileContext {
                    // use the relative path as key
                    path: file_name.clone(),
                    symbols,
                };
                Some(file_context)
            }
            "js" | "jsx" => {
                let symbols = Extractor::JavaScript.extract(file_name, file_content);
                let file_context = FileContext {
                    // use the relative path as key
                    path: file_name.clone(),
                    symbols,
                };
                Some(file_context)
            }
            "java" => {
                let symbols = Extractor::Java.extract(file_name, file_content);
                let file_context = FileContext {
                    // use the relative path as key
                    path: file_name.clone(),
                    symbols,
                };
                Some(file_context)
            }
            _ => None,
        };
    }

    fn extract_file_contexts(root: &String, files: Vec<String>) -> Vec<FileContext> {
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
        let file_contexts: Vec<FileContext> = filtered_files
            .par_iter()
            .map(|(each_file, file_path)| {
                pb.inc(1);
                return Graph::extract_file_context(each_file, file_path);
            })
            .filter(|ctx| ctx.is_some())
            .map(|ctx| ctx.unwrap())
            .collect();
        pb.finish_and_clear();
        file_contexts
    }

    fn build_global_symbol_table(
        file_contexts: &[FileContext],
    ) -> (HashMap<String, Vec<Symbol>>, HashMap<String, Vec<Symbol>>) {
        let mut global_def_symbol_table: HashMap<String, Vec<Symbol>> = HashMap::new();
        let mut global_ref_symbol_table: HashMap<String, Vec<Symbol>> = HashMap::new();

        file_contexts
            .iter()
            .flat_map(|file_context| file_context.symbols.iter())
            .for_each(|symbol| {
                if symbol.kind == SymbolKind::DEF {
                    global_def_symbol_table
                        .entry(symbol.name.clone())
                        .or_insert_with(Vec::new)
                        .push(symbol.clone());
                } else {
                    global_ref_symbol_table
                        .entry(symbol.name.clone())
                        .or_insert_with(Vec::new)
                        .push(symbol.clone());
                }
            });
        return (global_def_symbol_table, global_ref_symbol_table);
    }

    fn filter_pointless_symbols(
        file_contexts: &Vec<FileContext>,
        global_def_symbol_table: &HashMap<String, Vec<Symbol>>,
        global_ref_symbol_table: &HashMap<String, Vec<Symbol>>,
    ) -> Vec<FileContext> {
        let mut filtered_file_contexts = Vec::new();
        for file_context in file_contexts {
            let filtered_symbols = file_context
                .symbols
                .iter()
                .filter(|symbol| {
                    // ref but no def
                    if !global_def_symbol_table.contains_key(&symbol.name) {
                        return false;
                    }
                    return true;
                })
                .filter(|symbol| {
                    // def but no ref
                    if !global_ref_symbol_table.contains_key(&symbol.name) {
                        return true;
                    }
                    return true;
                })
                .map(|symbol| symbol.clone())
                .collect();

            filtered_file_contexts.push(FileContext {
                path: file_context.path.clone(),
                symbols: filtered_symbols,
            });
        }
        filtered_file_contexts
    }

    pub fn empty() -> Graph {
        return Graph {
            file_contexts: Vec::new(),
            _relation_graph: RelationGraph::new(),
            symbol_graph: SymbolGraph::new(),
        };
    }

    pub fn from(conf: GraphConfig) -> Graph {
        let start_time = Instant::now();
        // 1. call cupido
        // 2. extract symbols
        // 3. building def and ref relations
        let relation_graph = create_cupido_graph(&conf.project_path, conf.depth);
        let size = relation_graph.size();
        info!("relation graph ready, size: {:?}", size);

        let files = relation_graph.files();
        let file_len = files.len();
        let file_contexts = Self::extract_file_contexts(&conf.project_path, files);
        info!("symbol extract finished, files: {}", file_contexts.len());

        // filter pointless REF
        let (global_def_symbol_table, global_ref_symbol_table) =
            Self::build_global_symbol_table(&file_contexts);
        let final_file_contexts = Self::filter_pointless_symbols(
            &file_contexts,
            &global_def_symbol_table,
            &global_ref_symbol_table,
        );

        // building graph
        // 1. file - symbols
        // 2. symbols - symbols
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
        // commit cache
        let mut file_commit_cache: HashMap<String, HashSet<String>> = HashMap::new();
        let mut commit_file_cache: HashMap<String, HashSet<String>> = HashMap::new();
        let mut related_commits = |f: String| -> HashSet<String> {
            return if let Some(ref_commits) = file_commit_cache.get(&f) {
                ref_commits.clone()
            } else {
                let file_commits: HashSet<String> = relation_graph
                    .file_related_commits(&f)
                    .unwrap()
                    .into_iter()
                    .filter(|each| {
                        // reduce the impact of large commits
                        // large commit: edit more than 50% files once
                        return if let Some(ref_files) = commit_file_cache.get(each) {
                            ref_files.len() < file_len / 2
                        } else {
                            let ref_files: HashSet<String> = relation_graph
                                .commit_related_files(each)
                                .unwrap()
                                .into_iter()
                                .collect();

                            commit_file_cache.insert(each.clone(), ref_files.clone());
                            ref_files.len() < file_len / 2
                        };
                    })
                    .into_iter()
                    .collect();

                file_commit_cache.insert(f.clone(), file_commits.clone());
                file_commits
            };
        };

        let mut commit_file_cache2: HashMap<String, HashSet<String>> = HashMap::new();
        for file_context in &final_file_contexts {
            pb.inc(1);
            let def_related_commits = related_commits(file_context.path.clone());
            for symbol in &file_context.symbols {
                if symbol.kind != SymbolKind::REF {
                    continue;
                }
                let defs = global_def_symbol_table.get(&symbol.name).unwrap();

                let mut ratio_map: BTreeMap<usize, Vec<&Symbol>> = BTreeMap::new();
                for def in defs {
                    let f = def.file.clone();
                    let ref_related_commits = related_commits(f);
                    // calc the diff of two set
                    let intersection: HashSet<String> = ref_related_commits
                        .intersection(&def_related_commits)
                        .cloned()
                        .collect();

                    let mut ratio = 0;
                    intersection.iter().for_each(|each| {
                        // different range commits should have different scores
                        // large commit has less score

                        if let Some(ref_files) = commit_file_cache2.get(each) {
                            ratio += file_len - ref_files.len();
                        } else {
                            let ref_files: HashSet<String> = relation_graph
                                .commit_related_files(each)
                                .unwrap()
                                .into_iter()
                                .collect();
                            commit_file_cache2.insert(each.clone(), ref_files.clone());
                            ratio += file_len - ref_files.len();
                        };
                    });

                    if ratio > 0 {
                        ratio_map.entry(ratio).or_insert(Vec::new()).push(def);
                        symbol_graph.link_symbol_to_symbol(&symbol, &def);
                        symbol_graph.enhance_symbol_to_symbol(&symbol.id(), &def.id(), ratio);
                    }
                }

                let mut def_count = 0;
                for (&ratio, defs) in ratio_map.iter().rev() {
                    for def in defs {
                        symbol_graph.link_symbol_to_symbol(&symbol, &def);
                        symbol_graph.enhance_symbol_to_symbol(&symbol.id(), &def.id(), ratio);

                        def_count += 1;
                        if def_count >= conf.def_limit {
                            break;
                        }
                    }
                    if def_count >= conf.def_limit {
                        break;
                    }
                }
            }
        }
        pb.finish_and_clear();

        info!(
            "symbol graph ready, nodes: {}, edges: {}",
            symbol_graph.symbol_mapping.len(),
            symbol_graph.g.edge_count(),
        );
        info!("total time cost: {:?}", start_time.elapsed());

        return Graph {
            file_contexts,
            _relation_graph: relation_graph,
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

    pub fn related_files(&self, file_name: &String) -> Vec<RelatedFileContext> {
        if !self.files().contains(file_name) {
            return Vec::new();
        }

        // find all the defs in this file
        // and tracking all the references and theirs
        let mut file_counter = HashMap::new();
        self.symbol_graph
            .list_definitions(file_name)
            .iter()
            .for_each(|def| {
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
            .for_each(|each_ref| {
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
                    defs: self.symbol_graph.list_definitions(k).len(),
                    refs: self.symbol_graph.list_references(k).len(),
                };
            })
            .collect::<Vec<_>>();
        contexts.sort_by_key(|context| Reverse(context.score));
        return contexts;
    }

    pub fn related_symbols(&self, symbol: &Symbol) -> HashMap<Symbol, usize> {
        return match symbol.kind {
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
        };
    }

    pub fn file_metadata(&self, file_name: &String) -> FileMetadata {
        let symbols = self
            .symbol_graph
            .list_symbols(file_name)
            .iter()
            .cloned()
            .collect();
        return FileMetadata { symbols };
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RelatedFileContext {
    pub name: String,
    pub score: usize,
    pub defs: usize,
    pub refs: usize,
}

#[derive(Serialize, Deserialize)]
pub struct FileMetadata {
    pub symbols: Vec<Symbol>,
}

fn create_cupido_graph(project_path: &String, depth: u32) -> RelationGraph {
    let mut conf = Config::default();
    conf.repo_path = project_path.parse().unwrap();
    conf.depth = depth;

    let collector = get_collector();
    let graph = collector.walk(conf);
    return graph;
}

#[derive(Clone)]
pub struct GraphConfig {
    pub project_path: String,

    // a ref can only belong to limit def
    pub def_limit: usize,

    // commit history search depth
    pub depth: u32,
}

impl GraphConfig {
    pub fn default() -> GraphConfig {
        return GraphConfig {
            project_path: String::from("."),
            def_limit: 1,
            depth: 10240,
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::{Graph, GraphConfig};
    use petgraph::visit::EdgeRef;
    use tracing::{debug, info};

    #[test]
    #[ignore]
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
            .for_each(|each| {
                g.symbol_graph
                    .list_references_by_definition(&each.id())
                    .iter()
                    .for_each(|(each_ref, weight)| {
                        debug!("{} ref in {}, weight {}", each.file, each_ref.file, weight);
                    });
            });
    }

    #[test]
    #[ignore]
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
            .for_each(|each| {
                debug!(
                    "{:?} {}: {}:{}",
                    each.kind, each.name, each.range.start_point.row, each.range.start_point.column
                )
            });
    }

    #[test]
    #[ignore]
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
