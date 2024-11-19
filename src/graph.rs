use crate::extractor::Extractor;
use crate::symbol::{Symbol, SymbolGraph, SymbolKind};
use cupido::collector::config::Collect;
use cupido::collector::config::{get_collector, Config};
use cupido::relation::graph::RelationGraph as CupidoRelationGraph;
use indicatif::ProgressBar;
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::Instant;
use pyo3::{pyclass, pymethods};
use tracing::{debug, info};

pub struct FileContext {
    pub path: String,
    pub symbols: Vec<Symbol>,
}

#[pyclass]
pub struct Graph {
    pub(crate) file_contexts: Vec<FileContext>,
    pub(crate) _relation_graph: CupidoRelationGraph,
    pub(crate) symbol_graph: SymbolGraph,
}

impl Graph {
    fn extract_file_context(
        file_name: &String,
        file_path: &String,
        symbol_limit: usize,
    ) -> Option<FileContext> {
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
        match file_extension.as_str() {
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
                let mut symbols = Extractor::Go.extract(file_name, file_content);
                if symbols.len() > symbol_limit {
                    symbols = symbols
                        .into_iter()
                        .filter(|each| each.name.chars().next().unwrap().is_uppercase())
                        .collect();
                }
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
            "kt" => {
                let symbols = Extractor::Kotlin.extract(file_name, file_content);
                let file_context = FileContext {
                    // use the relative path as key
                    path: file_name.clone(),
                    symbols,
                };
                Some(file_context)
            }
            "swift" => {
                let symbols = Extractor::Swift.extract(file_name, file_content);
                let file_context = FileContext {
                    // use the relative path as key
                    path: file_name.clone(),
                    symbols,
                };
                Some(file_context)
            }
            _ => None,
        }
    }

    fn extract_file_contexts(
        root: &String,
        files: Vec<String>,
        symbol_limit: usize,
    ) -> Vec<FileContext> {
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
                return Graph::extract_file_context(each_file, file_path, symbol_limit);
            })
            .filter(|ctx| ctx.is_some())
            .map(|ctx| ctx.unwrap())
            .filter(|ctx| ctx.symbols.len() < symbol_limit)
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
        (global_def_symbol_table, global_ref_symbol_table)
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
        Graph {
            file_contexts: Vec::new(),
            _relation_graph: CupidoRelationGraph::new(),
            symbol_graph: SymbolGraph::new(),
        }
    }

    pub fn from(conf: GraphConfig) -> Graph {
        let start_time = Instant::now();
        // 1. call cupido
        // 2. extract symbols
        // 3. building def and ref relations
        let relation_graph = create_cupido_graph(
            &conf.project_path,
            conf.depth,
            conf.exclude_author_regex,
            conf.exclude_commit_regex,
        );
        let size = relation_graph.size();
        info!("relation graph ready, size: {:?}", size);

        let mut files = relation_graph.files();
        if !conf.exclude_file_regex.is_empty() {
            let re = Regex::new(&conf.exclude_file_regex).expect("Invalid regex");
            files.retain(|file| !re.is_match(file));
        }

        let file_len = files.len();
        let file_contexts =
            Self::extract_file_contexts(&conf.project_path, files, conf.symbol_limit);
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
                        return if let Some(ref_files) = commit_file_cache.get(each) {
                            ref_files.len()
                                < ((file_len as f32) * conf.commit_size_limit_ratio) as usize
                        } else {
                            let ref_files: HashSet<String> = relation_graph
                                .commit_related_files(each)
                                .unwrap()
                                .into_iter()
                                .collect();

                            commit_file_cache.insert(each.clone(), ref_files.clone());
                            ref_files.len()
                                < ((file_len as f32) * conf.commit_size_limit_ratio) as usize
                        };
                    })
                    .into_iter()
                    .collect();

                file_commit_cache.insert(f.clone(), file_commits.clone());
                file_commits
            };
        };

        let mut symbol_mapping: HashMap<String, usize> = HashMap::new();
        let mut symbol_count = |f: &String, g: &SymbolGraph| -> usize {
            return if let Some(count) = symbol_mapping.get(f) {
                *count
            } else {
                let count = g.list_references(&f).len();
                symbol_mapping.insert(f.clone(), count);
                count
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

                // all the possible definitions of this reference
                let defs = global_def_symbol_table.get(&symbol.name).unwrap();

                let mut ratio_map: BTreeMap<usize, Vec<&Symbol>> = BTreeMap::new();
                for def in defs {
                    let f = def.file.clone();
                    let ref_related_commits = related_commits(f);
                    // calc the diff of two set
                    let commit_intersection: HashSet<String> = ref_related_commits
                        .intersection(&def_related_commits)
                        .cloned()
                        .collect();

                    let mut ratio = 0.0;
                    commit_intersection.iter().for_each(|each_commit| {
                        // different range commits should have different scores
                        // large commit has less score

                        // how many files has been referenced
                        if let Some(commit_ref_files) = commit_file_cache2.get(each_commit) {
                            ratio += (file_len - commit_ref_files.len()) as f64 / (file_len as f64);
                        } else {
                            let commit_ref_files: HashSet<String> = relation_graph
                                .commit_related_files(each_commit)
                                .unwrap()
                                .into_iter()
                                .collect();
                            commit_file_cache2
                                .insert(each_commit.clone(), commit_ref_files.clone());
                            ratio += (file_len - commit_ref_files.len()) as f64 / (file_len as f64);
                        };
                    });

                    if ratio > 0.0 {
                        // complex file has lower ratio
                        let ref_count_in_file = symbol_count(&def.file.clone(), &symbol_graph);
                        if ref_count_in_file > 0 {
                            ratio = ratio / ref_count_in_file as f64;
                        }
                        if ratio < 1.0 {
                            ratio = 1.0;
                        }

                        ratio_map
                            .entry(ratio as usize)
                            .or_insert(Vec::new())
                            .push(def);
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

        Graph {
            file_contexts,
            _relation_graph: relation_graph,
            symbol_graph,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[pyclass]
pub struct RelatedSymbol {
    #[pyo3(get)]
    pub(crate) symbol: Symbol,

    #[pyo3(get)]
    pub(crate) weight: usize,
}


fn create_cupido_graph(
    project_path: &String,
    depth: u32,
    exclude_author_regex: Option<String>,
    exclude_commit_regex: Option<String>,
) -> CupidoRelationGraph {
    let mut conf = Config::default();
    conf.repo_path = project_path.parse().unwrap();
    conf.depth = depth;
    conf.author_exclude_regex = exclude_author_regex;
    conf.commit_exclude_regex = exclude_commit_regex;

    let collector = get_collector();
    let graph = collector.walk(conf);
    graph
}

#[pyclass]
#[derive(Clone)]
pub struct GraphConfig {
    #[pyo3(get, set)]
    pub project_path: String,

    // a ref can only belong to limit def
    pub def_limit: usize,

    // commit size limit
    // reduce the impact of large commits
    // large commit: edit more than xx% files once
    // default to 1.0, do nothing
    // set to 0.3, means 30%
    pub commit_size_limit_ratio: f32,

    // commit history search depth
    #[pyo3(get, set)]
    pub depth: u32,

    // symbol limit of each file
    pub symbol_limit: usize,

    #[pyo3(get, set)]
    pub exclude_file_regex: String,
    #[pyo3(get, set)]
    pub exclude_author_regex: Option<String>,
    #[pyo3(get, set)]
    pub exclude_commit_regex: Option<String>,
}

#[pymethods]
impl GraphConfig {
    #[new]
    pub fn default() -> GraphConfig {
        GraphConfig {
            project_path: String::from("."),
            def_limit: 1,
            commit_size_limit_ratio: 1.0,
            depth: 10240,
            symbol_limit: 4096,
            exclude_file_regex: String::new(),
            exclude_author_regex: None,
            exclude_commit_regex: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::{Graph, GraphConfig};
    use crate::symbol::DefRefPair;
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
        let files = g.related_files(String::from(
            "tree-sitter-stack-graphs/src/cli/util/reporter.rs",
        ));
        files.iter().for_each(|item| {
            info!("{}: {}", item.name, item.score);
        });
    }

    #[test]
    fn paths() {
        tracing_subscriber::fmt::init();
        let mut config = GraphConfig::default();
        config.project_path = String::from(".");
        let g = Graph::from(config);
        let symbols: Vec<DefRefPair> = g.pairs_between_files(
            String::from("src/extractor.rs"),
            String::from("src/graph.rs"),
        );
        symbols.iter().for_each(|pair| {
            info!(
                "{} {} {} -> {} {} {}",
                pair.src_symbol.file,
                pair.src_symbol.name,
                pair.src_symbol.range.start_point.row,
                pair.dst_symbol.file,
                pair.dst_symbol.name,
                pair.dst_symbol.range.start_point.row
            );
        });
    }
}
