/*
File: graph.rs
Functionality: Core graph construction and management logic.
Role: Manages the high-level Graph structure, integrating commit history and symbol relationships to build a comprehensive map of code dependencies.
*/
use crate::extractor::Extractor;
use crate::symbol::{Symbol, SymbolGraph, SymbolKind};
use anyhow::{Context, Result};
use cupido::collector::config::Collect;
use cupido::collector::config::{get_collector, Config};
use cupido::relation::graph::RelationGraph as CupidoRelationGraph;
use git2::Repository;
use indicatif::ProgressBar;
use pyo3::{pyclass, pymethods};
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info};

pub struct FileContext {
    pub path: Arc<String>,
    pub symbols: Vec<Symbol>,
}

pub struct NamespaceManager<'a> {
    namespaces: Vec<&'a Symbol>,
}

impl<'a> NamespaceManager<'a> {
    pub fn new(namespaces: Vec<&'a Symbol>) -> Self {
        NamespaceManager { namespaces }
    }

    pub fn get_line_depth(&self, line: usize) -> usize {
        let mut depth = 0;
        for namespace in &self.namespaces {
            if namespace.range.start_point.row < line && line < namespace.range.end_point.row {
                depth += 1;
            }
        }
        depth
    }
}

#[pyclass]
pub struct Graph {
    pub(crate) file_contexts: Vec<FileContext>,
    pub(crate) _relation_graph: CupidoRelationGraph,
    pub(crate) symbol_graph: SymbolGraph,
}

impl Graph {
    fn extract_file_context(
        file_name: Arc<String>,
        file_content: &String,
        _symbol_limit: usize,
    ) -> Option<FileContext> {
        let file_extension = match file_name.split('.').last() {
            Some(ext) => ext.to_lowercase(),
            None => {
                debug!("File {} has no extension, skipping...", file_name);
                return None;
            }
        };

        let extractor_mapping: HashMap<&str, &Extractor> = [
            ("rs", &Extractor::Rust),
            ("ts", &Extractor::TypeScript),
            ("tsx", &Extractor::TypeScript),
            ("go", &Extractor::Go),
            ("py", &Extractor::Python),
            ("js", &Extractor::JavaScript),
            ("jsx", &Extractor::JavaScript),
            ("java", &Extractor::Java),
            ("kt", &Extractor::Kotlin),
            ("swift", &Extractor::Swift),
            ("cs", &Extractor::CSharp),
        ]
        .into_iter()
        .collect();

        if let Some(extractor) = extractor_mapping.get(file_extension.as_str()) {
            let symbols = extractor.extract(file_name.clone(), file_content);
            let mut file_context = FileContext {
                // use the relative path as key
                path: file_name,
                symbols,
            };

            // further steps
            let rule = extractor.get_rule();
            if rule.namespace_filter_level == 0 {
                // do not filter
                return Some(file_context);
            }

            // start namespace pruning
            let namespaces: Vec<_> = file_context
                .symbols
                .iter()
                .filter(|symbol| symbol.kind == SymbolKind::NAMESPACE)
                .collect();

            if namespaces.is_empty() {
                return Some(file_context);
            }

            let namespace_manager = NamespaceManager::new(namespaces);
            file_context.symbols = file_context
                .symbols
                .iter()
                .filter_map(|symbol| {
                    if symbol.kind == SymbolKind::NAMESPACE {
                        return None;
                    }

                    let line = symbol.range.start_point.row;
                    let depth = namespace_manager.get_line_depth(line);

                    match symbol.kind {
                        SymbolKind::DEF => {
                            // nested def
                            if depth >= rule.namespace_filter_level {
                                return None;
                            }

                            return Some(symbol);
                        }
                        _ => Some(symbol),
                    }
                })
                .map(|f| f.clone())
                .collect();

            Some(file_context)
        } else {
            None
        }
    }

    fn extract_file_contexts(
        root: &String,
        commit_id: Option<String>,
        files: Vec<String>,
        symbol_limit: usize,
    ) -> Result<Vec<FileContext>> {
        let pb = ProgressBar::new(files.len() as u64);
        let file_contexts: Vec<FileContext> = files
            .into_par_iter()
            .filter_map(|file_path| {
                pb.inc(1);
                // Open repo in each thread to avoid Sync issues and keep it simple
                let repo = match Repository::open(root) {
                    Ok(r) => r,
                    Err(_) => return None,
                };

                let commit = if let Some(ref cid) = commit_id {
                    repo.revparse_single(cid)
                        .ok()
                        .and_then(|obj| obj.peel_to_commit().ok())?
                } else {
                    repo.head()
                        .ok()
                        .and_then(|head| head.peel_to_commit().ok())?
                };

                let tree = match commit.tree() {
                    Ok(t) => t,
                    Err(e) => {
                        debug!("Failed to get tree: {}", e);
                        return None;
                    }
                };
                let tree_entry = match tree.get_path(Path::new(&file_path)) {
                    Ok(entry) => entry,
                    Err(e) => {
                        debug!("Failed to get tree entry for {}: {}", file_path, e);
                        return None;
                    }
                };
                let object = match tree_entry.to_object(&repo) {
                    Ok(obj) => obj,
                    Err(e) => {
                        debug!("Failed to get object for {}: {}", file_path, e);
                        return None;
                    }
                };
                let blob = match object.peel_to_blob() {
                    Ok(blob) => blob,
                    Err(e) => {
                        debug!("Failed to peel blob for {}: {}", file_path, e);
                        return None;
                    }
                };

                if blob.is_binary() {
                    return None;
                }

                let content = match std::str::from_utf8(blob.content()) {
                    Ok(c) => c.to_string(),
                    Err(e) => {
                        debug!("Invalid UTF-8 content in {}: {}", file_path, e);
                        return None;
                    }
                };
                Graph::extract_file_context(Arc::new(file_path), &content, symbol_limit)
            })
            .filter(|ctx| ctx.symbols.len() < symbol_limit)
            .collect();

        pb.finish_and_clear();
        Ok(file_contexts)
    }

    fn build_global_symbol_table(
        file_contexts: &[FileContext],
    ) -> (
        HashMap<Arc<String>, Vec<Symbol>>,
        HashMap<Arc<String>, Vec<Symbol>>,
        HashMap<Arc<String>, Vec<Symbol>>,
    ) {
        let mut global_def_symbol_table: HashMap<Arc<String>, Vec<Symbol>> = HashMap::new();
        let mut global_ref_symbol_table: HashMap<Arc<String>, Vec<Symbol>> = HashMap::new();

        file_contexts
            .iter()
            .flat_map(|file_context| file_context.symbols.iter())
            .for_each(|symbol| {
                match symbol.kind {
                    SymbolKind::DEF => {
                        global_def_symbol_table
                            .entry(symbol.name.clone())
                            .or_insert_with(Vec::new)
                            .push(symbol.clone());
                    }
                    SymbolKind::REF => {
                        global_ref_symbol_table
                            .entry(symbol.name.clone())
                            .or_insert_with(Vec::new)
                            .push(symbol.clone());
                    }
                    // ignore
                    SymbolKind::NAMESPACE => {}
                }
            });

        let global_unique_def_symbol_table: HashMap<_, _> = global_def_symbol_table
            .iter()
            .filter(|(_, symbols)| symbols.len() == 1)
            .map(|(name, symbols)| (name.clone(), symbols.clone()))
            .collect();

        (
            global_def_symbol_table,
            global_ref_symbol_table,
            global_unique_def_symbol_table,
        )
    }

    fn filter_pointless_symbols(
        mut file_contexts: Vec<FileContext>,
        global_def_symbol_table: &HashMap<Arc<String>, Vec<Symbol>>,
        global_ref_symbol_table: &HashMap<Arc<String>, Vec<Symbol>>,
        symbol_len_limit: usize,
    ) -> Vec<FileContext> {
        for file_context in &mut file_contexts {
            file_context.symbols.retain(|symbol| {
                if symbol.name.len() <= symbol_len_limit {
                    return false;
                }
                match symbol.kind {
                    SymbolKind::DEF => global_ref_symbol_table.contains_key(&symbol.name),
                    SymbolKind::REF => global_def_symbol_table.contains_key(&symbol.name),
                    SymbolKind::NAMESPACE => true,
                }
            });
        }
        file_contexts
    }

    pub fn empty() -> Graph {
        Graph {
            file_contexts: Vec::new(),
            _relation_graph: CupidoRelationGraph::new(),
            symbol_graph: SymbolGraph::new(),
        }
    }

    pub fn from(conf: GraphConfig) -> Result<Graph> {
        let start_time = Instant::now();

        // 1. Building relation graph from git history
        let relation_graph = create_cupido_graph(
            &conf.project_path,
            conf.depth,
            conf.exclude_author_regex,
            conf.exclude_commit_regex,
            conf.issue_regex,
        )
        .context("Failed to create relation graph")?;
        info!(
            "relation graph ready, size: {:?}",
            relation_graph.size()
        );

        // 2. Filter files
        let mut files = relation_graph.files();
        if !conf.exclude_file_regex.is_empty() {
            let re = Regex::new(&conf.exclude_file_regex).context("Invalid exclude_file_regex")?;
            files.retain(|file| !re.is_match(file));
        }
        let file_len = files.len();

        // 3. Extract symbols from files
        let file_contexts = Self::extract_file_contexts(
            &conf.project_path,
            conf.commit_id.clone(),
            files,
            conf.symbol_limit,
        )?;
        info!("symbol extract finished, files: {}", file_contexts.len());

        // 4. Build global tables and filter pointless symbols
        let (global_def_table, global_ref_table, global_unique_def_table) =
            Self::build_global_symbol_table(&file_contexts);
        let final_file_contexts = Self::filter_pointless_symbols(
            file_contexts,
            &global_def_table,
            &global_ref_table,
            conf.symbol_len_limit,
        );

        // 5. Initialize symbol graph with files and symbols
        info!("start building symbol graph ...");
        let mut symbol_graph = SymbolGraph::new();
        for file_context in &final_file_contexts {
            symbol_graph.add_file(file_context.path.clone());
            for symbol in &file_context.symbols {
                symbol_graph.add_symbol(symbol.clone());
                symbol_graph.link_file_to_symbol(&file_context.path, symbol);
            }
        }

        // 6. Link symbols by commit history
        let pb = ProgressBar::new(final_file_contexts.len() as u64);

        // Pre-filter valid commits to avoid re-calculating for every file
        // A commit is valid if it doesn't touch too many files
        let all_commits = relation_graph.commits();
        let valid_commits: HashSet<String> = all_commits
            .into_iter()
            .filter(|c| {
                let touched = relation_graph.commit_related_files(c).unwrap_or_default();
                touched.len() < ((file_len as f32) * conf.commit_size_limit_ratio) as usize
            })
            .collect();

        let mut file_valid_commits_cache: HashMap<String, HashSet<String>> = HashMap::new();
        let mut get_file_valid_commits = |f: &str| -> HashSet<String> {
            let f_string = f.to_string();
            file_valid_commits_cache
                .entry(f_string.clone())
                .or_insert_with(|| {
                    relation_graph
                        .file_related_commits(&f_string)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|c| valid_commits.contains(c))
                        .collect()
                })
                .clone()
        };

        let mut symbol_mapping: HashMap<Arc<String>, usize> = HashMap::new();
        let mut get_symbol_count = |f: &Arc<String>, g: &SymbolGraph| -> usize {
            *symbol_mapping
                .entry(f.clone())
                .or_insert_with(|| g.list_references(f).len())
        };

        for file_context in &final_file_contexts {
            pb.inc(1);
            let def_related_commits = get_file_valid_commits(file_context.path.as_ref());
            if def_related_commits.is_empty() {
                continue;
            }

            for symbol in &file_context.symbols {
                if symbol.kind != SymbolKind::REF {
                    continue;
                }

                let defs = match global_def_table.get(&symbol.name) {
                    Some(defs) => defs,
                    None => continue,
                };

                let mut ratio_map: BTreeMap<usize, Vec<&Symbol>> = BTreeMap::new();
                for def in defs {
                    let ref_related_commits = get_file_valid_commits(def.file.as_ref());
                    let commit_intersection_count = def_related_commits
                        .iter()
                        .filter(|c| ref_related_commits.contains(*c))
                        .count();

                    if commit_intersection_count > 0 {
                        // Calc ratio (score) based on intersection
                        let mut score = 0.0;
                        // For each common commit, give more weight if the commit is "small"
                        for common_commit in def_related_commits.iter().filter(|c| ref_related_commits.contains(*c)) {
                            let touched_files_count = relation_graph.commit_related_files(common_commit).unwrap_or_default().len();
                            score += (file_len - touched_files_count) as f64 / (file_len as f64);
                        }

                        // Adjust score by file complexity
                        let ref_count_in_file = get_symbol_count(&def.file, &symbol_graph);
                        if ref_count_in_file > 0 {
                            score /= ref_count_in_file as f64;
                        }
                        if score < 1.0 {
                            score = 1.0;
                        }

                        ratio_map
                            .entry(score as usize)
                            .or_insert_with(Vec::new)
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

        // 7. Fallback for symbols without refs
        for file_context in &final_file_contexts {
            for each_def in &file_context.symbols {
                if each_def.kind != SymbolKind::DEF {
                    continue;
                }
                
                if symbol_graph.list_references_by_definition(&each_def.id()).is_empty() {
                    if let Some(fallback_defs) = global_unique_def_table.get(&each_def.name) {
                        for fallback_def in fallback_defs {
                            if let Some(refs) = global_ref_table.get(&each_def.name) {
                                for r in refs {
                                    symbol_graph.link_symbol_to_symbol(fallback_def, r);
                                }
                            }
                        }
                    }
                }
            }
        }

        info!(
            "symbol graph ready, nodes: {}, edges: {}",
            symbol_graph.symbol_mapping.len(),
            symbol_graph.g.edge_count(),
        );
        info!("total time cost: {:?}", start_time.elapsed());

        Ok(Graph {
            file_contexts: final_file_contexts,
            _relation_graph: relation_graph,
            symbol_graph,
        })
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[pyclass]
pub struct RelatedSymbol {
    #[pyo3(get)]
    pub symbol: Symbol,

    #[pyo3(get)]
    pub weight: usize,
}

fn create_cupido_graph(
    project_path: &String,
    depth: u32,
    exclude_author_regex: Option<String>,
    exclude_commit_regex: Option<String>,
    issue_regex: Option<String>,
) -> Result<CupidoRelationGraph> {
    let mut conf = Config::default();
    conf.repo_path = project_path
        .parse()
        .context(format!("Invalid project path: {}", project_path))?;
    conf.depth = depth;
    conf.author_exclude_regex = exclude_author_regex;
    conf.commit_exclude_regex = exclude_commit_regex;
    if let Some(re) = issue_regex {
        conf.issue_regex = re;
    }

    let collector = get_collector();
    let graph = collector.walk(conf);
    Ok(graph)
}

#[pyclass]
#[derive(Clone)]
pub struct GraphConfig {
    #[pyo3(get, set)]
    pub project_path: String,

    // if a def has been referenced over `def_limit` times, it will be ignored.
    #[pyo3(get, set)]
    pub def_limit: usize,

    // commit size limit
    // reduce the impact of large commits
    // large commit: edit more than xx% files once
    // default to 1.0, do nothing
    // set to 0.3, means 30%
    #[pyo3(get, set)]
    pub commit_size_limit_ratio: f32,

    // commit history search depth
    #[pyo3(get, set)]
    pub depth: u32,

    // symbol limit of each file, for ignoring large files
    #[pyo3(get, set)]
    pub symbol_limit: usize,

    // if a symbol len <= `symbol_len_limit`, it will be ignored.
    #[pyo3(get, set)]
    pub symbol_len_limit: usize,

    #[pyo3(get, set)]
    pub exclude_file_regex: String,
    #[pyo3(get, set)]
    pub exclude_author_regex: Option<String>,
    #[pyo3(get, set)]
    pub exclude_commit_regex: Option<String>,

    #[pyo3(get, set)]
    pub issue_regex: Option<String>,

    #[pyo3(get, set)]
    pub commit_id: Option<String>,
}

#[pymethods]
impl GraphConfig {
    #[new]
    pub fn default() -> GraphConfig {
        GraphConfig {
            project_path: String::from("."),
            def_limit: 16,
            commit_size_limit_ratio: 1.0,
            depth: 10240,
            symbol_limit: 4096,
            symbol_len_limit: 0,
            exclude_file_regex: String::new(),
            exclude_author_regex: None,
            exclude_commit_regex: None,
            issue_regex: None,
            commit_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{DefRefPair, Symbol};
    use petgraph::visit::EdgeRef;
    use std::sync::Arc;
    use tracing::{debug, info};

    #[test]
    #[ignore]
    fn symbol_graph_rust() {
        tracing_subscriber::fmt::init();
        let mut config = GraphConfig::default();
        config.project_path = String::from("../stack-graphs");
        let g = Graph::from(config).unwrap();
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
            .list_definitions(&Arc::new(String::from(
                "tree-sitter-stack-graphs/src/cli/util/reporter.rs",
            )))
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
        let g = Graph::from(config).unwrap();
        g.file_contexts.iter().for_each(|context| {
            debug!("{}: {:?}", context.path, context.symbols);
        });

        g.symbol_graph
            .list_symbols(&Arc::new(String::from("lsif/src/main.ts")))
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
        let g = Graph::from(config).unwrap();
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
        let g = Graph::from(config).unwrap();
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

        let issues = g.list_file_issues(String::from("src/extractor.rs"));
        let commits = g.list_file_commits(String::from("src/graph.rs"));
        assert!(issues.len() > 0);
        assert!(commits.len() > 0);
    }

    #[test]
    fn test_graph_config_filters() {
        let mut config = GraphConfig::default();
        config.project_path = String::from(".");
        // exclude all rs files should result in no files in graph
        config.exclude_file_regex = String::from(".*\\.rs$");
        let g = Graph::from(config).unwrap();

        let files = g.files();
        for file in files {
            assert!(
                !file.ends_with(".rs"),
                "File {} should have been excluded",
                file
            );
        }
    }

    #[test]
    fn test_graph_with_commit_id() {
        let mut config = GraphConfig::default();
        config.project_path = String::from(".");
        // use an old commit
        config.commit_id = Some(String::from("f034426"));
        let g = Graph::from(config).unwrap();
        assert!(!g.file_contexts.is_empty());
        // f034426 should have some files
        info!("Files in commit f034426: {}", g.file_contexts.len());
    }

    #[test]
    fn test_internal_symbol_filtering() {
        let file_a = Arc::new("a.rs".to_string());
        let file_b = Arc::new("b.rs".to_string());
        let range = tree_sitter::Range {
            start_byte: 0,
            end_byte: 0,
            start_point: tree_sitter::Point { row: 0, column: 0 },
            end_point: tree_sitter::Point { row: 0, column: 0 },
        };

        // Case 1: DEF "foo" in A, REF "foo" in B -> both should be kept
        let def_foo = Symbol::new_def(file_a.clone(), Arc::new("foo".to_string()), range);
        let ref_foo = Symbol::new_ref(file_b.clone(), Arc::new("foo".to_string()), range);

        // Case 2: DEF "bar" in A, no REF -> DEF "bar" should be filtered out
        let def_bar = Symbol::new_def(file_a.clone(), Arc::new("bar".to_string()), range);

        // Case 3: REF "baz" in B, no DEF -> REF "baz" should be filtered out
        let ref_baz = Symbol::new_ref(file_b.clone(), Arc::new("baz".to_string()), range);

        let contexts = vec![
            FileContext {
                path: file_a.clone(),
                symbols: vec![def_foo.clone(), def_bar.clone()],
            },
            FileContext {
                path: file_b.clone(),
                symbols: vec![ref_foo.clone(), ref_baz.clone()],
            },
        ];

        let (global_def, global_ref, _) = Graph::build_global_symbol_table(&contexts);
        let filtered = Graph::filter_pointless_symbols(contexts, &global_def, &global_ref, 0);

        let symbols_a = &filtered[0].symbols;
        let symbols_b = &filtered[1].symbols;

        assert!(symbols_a.iter().any(|s| s.name.as_ref() == "foo"));
        assert!(!symbols_a.iter().any(|s| s.name.as_ref() == "bar"));
        assert!(symbols_b.iter().any(|s| s.name.as_ref() == "foo"));
        assert!(!symbols_b.iter().any(|s| s.name.as_ref() == "baz"));
    }

    #[test]
    fn test_fqn_isolation_in_graph() {
        let file_a = Arc::new("Auth.java".to_string());
        let file_b = Arc::new("Data.java".to_string());
        let range = tree_sitter::Range {
            start_byte: 0,
            end_byte: 0,
            start_point: tree_sitter::Point { row: 0, column: 0 },
            end_point: tree_sitter::Point { row: 0, column: 0 },
        };

        // AuthService.validate vs DataService.validate
        let def_auth = Symbol::new_def(file_a.clone(), Arc::new("AuthService.validate".to_string()), range);
        let ref_data = Symbol::new_ref(file_b.clone(), Arc::new("DataService.validate".to_string()), range);
        
        let contexts = vec![
            FileContext {
                path: file_a.clone(),
                symbols: vec![def_auth.clone()],
            },
            FileContext {
                path: file_b.clone(),
                symbols: vec![ref_data.clone()],
            },
        ];

        // 构建全局符号表
        let (global_def, _, _) = Graph::build_global_symbol_table(&contexts);
        
        // 核心验证：ref_data ("DataService.validate") 尝试寻找定义
        // 应该找不到，因为它不匹配 def_auth ("AuthService.validate")
        assert!(!global_def.contains_key(&ref_data.name));
        assert!(global_def.contains_key(&def_auth.name));
    }
}
