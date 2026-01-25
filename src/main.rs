/*
File: main.rs
Functionality: Command-line interface (CLI) application entry point.
Role: Provides the binary executable with various subcommands for graph analysis, server management, and integration.
*/
use anyhow::Context;
use clap::Parser;
use csv::Writer;
use git2::{Commit, DiffOptions, Error, Object, ObjectType, Repository};
use gossiphs::api::RelatedFileContext;
use gossiphs::graph::{Graph, GraphConfig};
use gossiphs::server::{server_main, ServerConfig};
use indicatif::ProgressBar;
use inquire::Text;
use rayon::iter::ParallelIterator;
use rayon::prelude::IntoParallelRefIterator;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use termtree::Tree;
use tracing::{debug, info};

#[derive(Parser, Debug)]
#[clap(
    name = "gossiphs",
    bin_name = "gossiphs",
    version = env!("CARGO_PKG_VERSION"),
    about = "gossiphs (gossip-graphs) command line tool",
)]
struct Cli {
    #[clap(subcommand)]
    cmd: SubCommand,
}

#[derive(Parser, Debug)]
enum SubCommand {
    #[clap(name = "relate")]
    Relate(RelateCommand),

    #[clap(name = "relation")]
    Relation(RelationCommand),

    #[clap(name = "relation2")]
    Relation2(RelationCommand),

    #[clap(name = "interactive")]
    Interactive(InteractiveCommand),

    #[clap(name = "server")]
    Server(ServerCommand),

    #[clap(name = "obsidian")]
    Obsidian(ObsidianCommand),

    /// Diff analysis (without checkout)
    #[clap(name = "diff")]
    Diff(DiffCommand),
}

#[derive(Parser, Debug)]
struct CommonOptions {
    #[clap(short, long)]
    #[clap(default_value = ".")]
    project_path: String,

    /// precise-first analysis
    #[clap(long)]
    #[clap(default_value = "false")]
    strict: bool,

    #[clap(long)]
    def_limit: Option<usize>,

    /// git commit history search depth
    #[clap(long)]
    depth: Option<u32>,

    #[clap(long)]
    exclude_file_regex: Option<String>,

    #[clap(long)]
    exclude_author_regex: Option<String>,

    #[clap(long)]
    symbol_len_limit: Option<usize>,

    /// Output-level filtering: keep at least N related files per file (0 disables filtering)
    #[clap(long)]
    file_min_links: Option<usize>,

    /// Output-level filtering: keep at most N related files per file (0 disables filtering)
    #[clap(long)]
    file_max_links: Option<usize>,
}

impl CommonOptions {
    #[cfg(test)]
    fn default() -> CommonOptions {
        CommonOptions {
            project_path: String::from("."),
            strict: false,
            def_limit: None,
            depth: None,
            exclude_file_regex: None,
            exclude_author_regex: None,
            symbol_len_limit: None,
            file_min_links: None,
            file_max_links: None,
        }
    }
}

fn apply_output_filter_options(config: &mut GraphConfig, opts: &CommonOptions) {
    if let Some(n) = opts.file_min_links {
        config.file_min_links = n;
    }
    if let Some(n) = opts.file_max_links {
        config.file_max_links = n;
    }
}

#[derive(Parser, Debug)]
struct RelateCommand {
    #[clap(flatten)]
    common_options: CommonOptions,

    #[clap(long)]
    #[clap(default_value = "")]
    file: String,

    #[clap(long)]
    #[clap(default_value = "")]
    file_txt: String,

    #[clap(long)]
    #[clap(default_value = None)]
    json: Option<String>,

    #[clap(long)]
    #[clap(default_value = "true")]
    ignore_zero: bool,
}

#[derive(Parser, Debug)]
struct RelationCommand {
    #[clap(flatten)]
    common_options: CommonOptions,

    #[clap(long)]
    #[clap(default_value = "output.csv")]
    csv: String,

    #[clap(long)]
    #[clap(default_value = "")]
    symbol_csv: String,

    #[clap(long)]
    #[clap(default_value = "output.index")]
    index_file: String,
}

#[derive(Parser, Debug)]
struct InteractiveCommand {
    #[clap(flatten)]
    common_options: CommonOptions,

    #[clap(long)]
    #[clap(default_value = "false")]
    dry: bool,
}

#[derive(Parser, Debug)]
struct ServerCommand {
    #[clap(flatten)]
    common_options: CommonOptions,

    #[clap(long)]
    #[clap(default_value = "9411")]
    port: u16,
}

#[derive(Parser, Debug)]
struct ObsidianCommand {
    #[clap(flatten)]
    common_options: CommonOptions,

    #[clap(long)]
    vault_dir: String,
}

#[derive(Parser, Debug)]
struct DiffCommand {
    #[clap(flatten)]
    common_options: CommonOptions,

    #[clap(long)]
    #[clap(default_value = "HEAD~1")]
    target: String,

    #[clap(long)]
    #[clap(default_value = "HEAD")]
    source: String,

    /// use json format for output, else use tree
    #[clap(long)]
    #[clap(default_value = "false")]
    json: bool,
}

impl RelateCommand {
    pub fn get_files(&self) -> Vec<String> {
        if !self.file_txt.is_empty() {
            let file_contents = match fs::read_to_string(&self.file_txt) {
                Ok(contents) => contents,
                Err(err) => {
                    eprintln!("Error reading file {}: {}", self.file_txt, err);
                    return Vec::new();
                }
            };
            return file_contents
                .clone()
                .lines()
                .filter(|each| !each.trim().is_empty())
                .map(|each| each.to_string())
                .collect();
        }
        self.file.split(';').map(|each| each.to_string()).collect()
    }
}

fn main() -> anyhow::Result<()> {
    let cli: Cli = Cli::parse();

    match cli.cmd {
        SubCommand::Relate(search_cmd) => handle_relate(search_cmd)?,
        SubCommand::Relation(relation_cmd) => handle_relation(relation_cmd)?,
        SubCommand::Relation2(relation_cmd) => handle_relation_v2(relation_cmd)?,
        SubCommand::Interactive(interactive_cmd) => handle_interactive(interactive_cmd)?,
        SubCommand::Server(server_cmd) => handle_server(server_cmd)?,
        SubCommand::Obsidian(obsidian_cmd) => handle_obsidian(obsidian_cmd)?,
        SubCommand::Diff(diff_cmd) => handle_diff(diff_cmd)?,
    }
    Ok(())
}

fn handle_relate(relate_cmd: RelateCommand) -> anyhow::Result<()> {
    // result will be saved to file, so enable log
    if relate_cmd.json.is_some() {
        let _ = tracing_subscriber::fmt::try_init();
    }
    let mut config = GraphConfig::default();
    config.project_path = relate_cmd.common_options.project_path.clone();
    if relate_cmd.common_options.strict {
        config.def_limit = 1
    }
    if let Some(depth) = relate_cmd.common_options.depth {
        config.depth = depth;
    }

    apply_output_filter_options(&mut config, &relate_cmd.common_options);

    let g = Graph::from(config).context("Failed to create graph")?;

    let mut related_files_data = Vec::new();
    let files = relate_cmd.get_files();
    for file in &files {
        let mut files = g.related_files(file.clone());
        if relate_cmd.ignore_zero {
            files.retain(|each| each.score > 0);
        }
        related_files_data.push(RelatedFileWrapper {
            name: file.to_string(),
            related: files,
        });
    }
    let json = serde_json::to_string(&related_files_data).context("Failed to serialize related files data")?;
    if let Some(json_path) = relate_cmd.json {
        fs::write(json_path, json).context("Failed to write JSON output")?;
    } else {
        println!("{}", json);
    }
    Ok(())
}

fn handle_relation_v2(relation_cmd: RelationCommand) -> anyhow::Result<()> {
    let mut config = GraphConfig::default();
    config.project_path = relation_cmd.common_options.project_path.clone();
    if relation_cmd.common_options.strict {
        config.def_limit = 1;
    }
    if let Some(def_limit) = relation_cmd.common_options.def_limit {
        config.def_limit = def_limit;
    }

    if let Some(depth) = relation_cmd.common_options.depth {
        config.depth = depth;
    }
    if let Some(ref exclude) = relation_cmd.common_options.exclude_file_regex {
        config.exclude_file_regex = exclude.clone();
    }
    config.exclude_author_regex = relation_cmd.common_options.exclude_author_regex.clone();

    apply_output_filter_options(&mut config, &relation_cmd.common_options);

    let g = Graph::from(config).context("Failed to create graph")?;
    let relation_list = g.list_all_relations();

    let mut writer =
        BufWriter::new(File::create(relation_cmd.index_file).context("Unable to create index file")?);
    for node in relation_list.file_nodes {
        let serialized = serde_json::to_string(&node).context("Failed to serialize FileNode")?;
        writeln!(writer, "{}", serialized).context("Unable to write data")?;
    }
    for relation in relation_list.file_relations {
        let serialized =
            serde_json::to_string(&relation).context("Failed to serialize FileRelation")?;
        writeln!(writer, "{}", serialized).context("Unable to write data")?;
    }
    for node in relation_list.symbol_nodes {
        let serialized = serde_json::to_string(&node).context("Failed to serialize SymbolNode")?;
        writeln!(writer, "{}", serialized).context("Unable to write data")?;
    }
    Ok(())
}

fn handle_relation(relation_cmd: RelationCommand) -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let mut config = GraphConfig::default();
    config.project_path = relation_cmd.common_options.project_path.clone();
    if relation_cmd.common_options.strict {
        config.def_limit = 1;
    }
    if let Some(def_limit) = relation_cmd.common_options.def_limit {
        config.def_limit = def_limit;
    }

    if let Some(depth) = relation_cmd.common_options.depth {
        config.depth = depth;
    }
    if let Some(ref exclude) = relation_cmd.common_options.exclude_file_regex {
        config.exclude_file_regex = exclude.clone();
    }
    config.exclude_author_regex = relation_cmd.common_options.exclude_author_regex.clone();
    if let Some(symbol_len_limit) = relation_cmd.common_options.symbol_len_limit {
        config.symbol_len_limit = symbol_len_limit;
    }

    apply_output_filter_options(&mut config, &relation_cmd.common_options);

    let g = Graph::from(config).context("Failed to create graph")?;

    let mut files: Vec<String> = g.files().into_iter().collect();
    files.sort();

    // Create a new CSV writer
    let wtr_result = Writer::from_path(relation_cmd.csv);
    let mut wtr = match wtr_result {
        Ok(writer) => writer,
        Err(e) => anyhow::bail!("Failed to create CSV writer: {}", e),
    };
    // Write the header row
    let mut header = vec!["".to_string()];
    header.extend(files.clone());
    wtr.write_record(&header).context("Failed to write CSV header")?;

    let mut symbol_wtr_opts = None;
    if !relation_cmd.symbol_csv.is_empty() {
        let symbol_wtr_result = Writer::from_path(relation_cmd.symbol_csv);
        symbol_wtr_opts = match symbol_wtr_result {
            Ok(writer) => Some(writer),
            Err(e) => anyhow::bail!("Failed to create CSV writer: {}", e),
        };
        let mut header = vec!["".to_string()];
        header.extend(files.clone());
        if let Some(symbol_wtr) = symbol_wtr_opts.as_mut() {
            symbol_wtr
                .write_record(&header)
                .context("Failed to write header to symbol_wtr")?;
        }
    }

    // Write each row
    let pb = ProgressBar::new(files.len() as u64);
    let results: HashMap<String, (std::collections::BTreeMap<usize, String>, std::collections::BTreeMap<usize, String>)> = files
        .par_iter()
        .map(|file| {
            pb.inc(1);
            let mut row = std::collections::BTreeMap::new();
            let mut pair_row = std::collections::BTreeMap::new();
            let related_files_map: HashMap<_, _> = g
                .related_files(file.clone())
                .into_iter()
                .map(|rf| (rf.name, rf.score))
                .collect();

            for (i, related_file) in files.iter().enumerate() {
                if let Some(score) = related_files_map.get(related_file) {
                    if *score > 0 {
                        row.insert(i, score.to_string());
                        if symbol_wtr_opts.is_some() {
                            let pairs = g
                                .pairs_between_files(file.clone(), related_file.clone())
                                .iter()
                                .map(|each| each.src_symbol.name.as_ref().clone())
                                .collect::<Vec<String>>();
                            pair_row.insert(i, pairs.join("|"));
                        }
                    }
                }
            }

            (file.clone(), (row, pair_row))
        })
        .collect();
    pb.finish_and_clear();

    // Sort results by the original order of files
    for file in &files {
        if let Some((row_map, pair_row_map)) = results.get(file) {
            let mut row = vec![file.clone()];
            let mut pair_row = vec![file.clone()];
            for i in 0..files.len() {
                row.push(row_map.get(&i).cloned().unwrap_or_default());
                pair_row.push(pair_row_map.get(&i).cloned().unwrap_or_default());
            }
            wtr.write_record(&row).context("Failed to write record")?;
            if let Some(symbol_wtr) = symbol_wtr_opts.as_mut() {
                symbol_wtr
                    .write_record(&pair_row)
                    .context("Failed to write pair_row to symbol_wtr")?;
            }
        }
    }

    // Flush the writer to ensure all data is written
    wtr.flush().context("Failed to flush CSV writer")?;
    Ok(())
}

fn handle_interactive(interactive_cmd: InteractiveCommand) -> anyhow::Result<()> {
    let mut config = GraphConfig::default();
    config.project_path = interactive_cmd.common_options.project_path.clone();
    if interactive_cmd.common_options.strict {
        config.def_limit = 1
    }
    if let Some(depth) = interactive_cmd.common_options.depth {
        config.depth = depth;
    }

    apply_output_filter_options(&mut config, &interactive_cmd.common_options);

    let g = Graph::from(config).context("Failed to create graph")?;

    if interactive_cmd.dry {
        return Ok(());
    }

    loop {
        let file_path_result = Text::new("File Path:").prompt();
        match file_path_result {
            Ok(name) => {
                let files = g.related_files(name.clone());
                let json = serde_json::to_string_pretty(&RelatedFileWrapper {
                    name,
                    related: files,
                })
                .context("Failed to serialize RelatedFileWrapper")?;
                println!("{}", json);
            }
            Err(_) => break,
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct RelatedFileWrapper {
    pub name: String,
    pub related: Vec<RelatedFileContext>,
}

#[derive(Serialize, Deserialize)]
struct DiffFileContext {
    pub name: String,
    pub added: Vec<RelatedFileContext>,
    pub deleted: Vec<RelatedFileContext>,
    pub modified: Vec<RelatedFileContext>,
}

fn handle_server(server_cmd: ServerCommand) -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let mut config = GraphConfig::default();
    config.project_path = server_cmd.common_options.project_path.clone();
    if server_cmd.common_options.strict {
        config.def_limit = 1
    }
    if let Some(depth) = server_cmd.common_options.depth {
        config.depth = depth;
    }

    apply_output_filter_options(&mut config, &server_cmd.common_options);

    let g = Graph::from(config).context("Failed to create graph")?;

    let mut server_config = ServerConfig::new(g);
    server_config.port = server_cmd.port;
    info!("server up, port: {}", server_config.port);
    server_main(server_config).context("Server execution failed")?;
    Ok(())
}

fn handle_obsidian(obsidian_cmd: ObsidianCommand) -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let mut config = GraphConfig::default();
    config.project_path = obsidian_cmd.common_options.project_path.clone();
    if obsidian_cmd.common_options.strict {
        config.def_limit = 1
    }
    if let Some(depth) = obsidian_cmd.common_options.depth {
        config.depth = depth;
    }

    apply_output_filter_options(&mut config, &obsidian_cmd.common_options);

    let g = Graph::from(config).context("Failed to create graph")?;

    // create mirror files
    // add links to files
    let files = g.files();
    fs::create_dir(&obsidian_cmd.vault_dir).context("Failed to create vault directory")?;

    for each_file in files {
        let related = g.related_files(each_file.clone());
        let markdown_filename = format!("{}/{}.md", &obsidian_cmd.vault_dir, each_file);
        let mut markdown_content = String::new();
        for related_file in related {
            markdown_content.push_str(&format!("[[{}]]\n", related_file.name));
        }

        let path = Path::new(&markdown_filename);
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).context("Failed to create parent directory")?;
        let mut file = File::create(&markdown_filename).context("Failed to create markdown file")?;
        file.write_all(markdown_content.as_bytes()).context("Failed to write to markdown file")?;
        debug!("Successfully wrote to {}", markdown_filename);
    }
    Ok(())
}

fn handle_diff(diff_cmd: DiffCommand) -> anyhow::Result<()> {
    // repo status check
    let project_path = diff_cmd.common_options.project_path.clone();
    let repo = Repository::open(&project_path).context("Failed to open repository")?;
    
    let (target_commit, _target_object) = get_commit_and_object(&repo, &diff_cmd.target).context("Failed to get target commit")?;
    let (source_commit, _source_object) = get_commit_and_object(&repo, &diff_cmd.source).context("Failed to get source commit")?;

    // gen graphs
    let mut config = GraphConfig::default();
    config.project_path = project_path;
    if diff_cmd.common_options.strict {
        config.def_limit = 1
    }
    if let Some(depth) = diff_cmd.common_options.depth {
        config.depth = depth;
    }

    apply_output_filter_options(&mut config, &diff_cmd.common_options);

    let mut target_config = config.clone();
    target_config.commit_id = Some(target_commit.id().to_string());
    let target_graph = Graph::from(target_config).context("Failed to create target graph")?;

    let mut source_config = config.clone();
    source_config.commit_id = Some(source_commit.id().to_string());
    let source_graph = Graph::from(source_config).context("Failed to create source graph")?;

    // diff files
    let mut diff_options = DiffOptions::new();
    let diff = repo
        .diff_tree_to_tree(
            Some(&target_commit.tree().context("Target commit tree missing")?),
            Some(&source_commit.tree().context("Source commit tree missing")?),
            Some(&mut diff_options),
        )
        .context("Diff failed")?;

    let mut diff_files: Vec<String> = Vec::new();
    diff.foreach(
        &mut |delta, _| {
            if let Some(new_file) = delta.new_file().path() {
                diff_files.push(String::from(new_file.to_str().unwrap()));
            }
            true
        },
        None,
        None,
        None,
    )
    .context("Diff traversal failed")?;

    // diff context
    let mut ret: Vec<DiffFileContext> = Vec::new();
    for each_file in diff_files {
        let target_related_map: HashMap<String, RelatedFileContext> = target_graph
            .related_files(each_file.clone())
            .into_iter()
            .map(|item| (item.name.clone(), item))
            .collect();
        let source_related_map: HashMap<String, RelatedFileContext> = source_graph
            .related_files(each_file.clone())
            .into_iter()
            .map(|item| (item.name.clone(), item))
            .collect();
        let mut added_links: Vec<RelatedFileContext> = Vec::new();
        let mut modified_links: Vec<RelatedFileContext> = Vec::new();
        for (_, item) in source_related_map.clone() {
            if !target_related_map.contains_key(&item.name) {
                added_links.push(item);
            } else {
                // both
                modified_links.push(item);
            }
        }
        let mut removed_links: Vec<RelatedFileContext> = Vec::new();
        for (_, item) in target_related_map.clone() {
            if !source_related_map.contains_key(&item.name) {
                removed_links.push(item);
            }
        }
        ret.push(DiffFileContext {
            name: each_file,
            added: added_links,
            deleted: removed_links,
            modified: modified_links,
        })
    }

    // output format
    if diff_cmd.json {
        let json = serde_json::to_string(&ret).context("Failed to serialize diff context")?;
        println!("{}", json);
    } else {
        for file_context in &ret {
            let file_name = &file_context.name;
            let mut file_node = Tree::new(file_name.as_str());

            let mut names = Vec::new();
            for link in &file_context.added {
                names.push(format!("{} (ADDED)", link.name));
            }
            for link in &file_context.deleted {
                names.push(format!("{} (DELETED)", link.name));
            }
            for link in &file_context.modified {
                names.push(format!("{}", link.name));
            }

            // Push the references of the prefixed names into the file_node
            for prefixed_name in &names {
                file_node.push(Tree::new(prefixed_name.as_str()));
            }

            println!("{}", file_node)
        }
    }
    Ok(())
}

fn get_commit_and_object<'repo>(
    repo: &'repo Repository,
    rev: &str,
) -> Result<(Commit<'repo>, Object<'repo>), Error> {
    let obj = repo.revparse_single(rev)?;

    // Check if the object is a commit or needs to be peeled to a commit
    let commit = if obj.kind() == Some(ObjectType::Commit) {
        obj.as_commit()
            .map(|commit| (commit.clone(), obj.clone()))
            .ok_or_else(|| Error::from_str("Object is not a commit"))
    } else {
        let peeled_obj = obj.peel(ObjectType::Commit)?;
        peeled_obj
            .as_commit()
            .map(|commit| (commit.clone(), peeled_obj.clone()))
            .ok_or_else(|| Error::from_str("Object could not be peeled to commit"))
    };

    commit
}

#[test]
fn test_handle_relate() {
    let relate_cmd = RelateCommand {
        common_options: CommonOptions::default(),
        file: "src/extractor.rs".to_string(),
        file_txt: "".to_string(),
        json: None,
        ignore_zero: true,
    };
    handle_relate(relate_cmd).unwrap();
}

#[test]
fn test_handle_relate_files() {
    let relate_cmd = RelateCommand {
        common_options: CommonOptions::default(),
        file: "src/extractor.rs;src/main.rs;src/graph.rs".to_string(),
        file_txt: "".to_string(),
        json: None,
        ignore_zero: true,
    };
    handle_relate(relate_cmd).unwrap();
}

#[test]
fn test_handle_relate_files_strict() {
    let relate_cmd = RelateCommand {
        common_options: CommonOptions::default(),
        file: "src/extractor.rs;src/rule.rs;src/main.rs;src/graph.rs".to_string(),
        file_txt: "".to_string(),
        json: None,
        ignore_zero: true,
    };
    handle_relate(relate_cmd).unwrap();
}

#[test]
#[ignore]
fn test_handle_relate_file_txt() {
    let relate_cmd = RelateCommand {
        common_options: CommonOptions::default(),
        file: "".to_string(),
        file_txt: "./aa.txt".to_string(),
        json: None,
        ignore_zero: true,
    };
    handle_relate(relate_cmd).unwrap();
}

#[test]
#[ignore]
fn server_test() {
    handle_server(ServerCommand {
        common_options: CommonOptions::default(),
        port: 9411,
    }).unwrap();
}

#[test]
#[ignore]
fn obsidian_test() {
    handle_obsidian(ObsidianCommand {
        common_options: CommonOptions::default(),
        vault_dir: "./vault".to_string(),
    }).unwrap();
}

#[test]
fn diff_test() {
    handle_diff(DiffCommand {
        common_options: CommonOptions::default(),
        target: "HEAD~10".to_string(),
        source: "HEAD".to_string(),
        json: false,
    }).unwrap();

    handle_diff(DiffCommand {
        common_options: CommonOptions::default(),
        target: "d18a5db39752d244664a23f74e174448b66b5b7e".to_string(),
        source: "HEAD".to_string(),
        json: false,
    }).unwrap();
}

#[test]
fn relation_test() {
    let mut config = CommonOptions::default();
    config.exclude_file_regex = Some("".parse().unwrap());
    config.project_path = ".".parse().unwrap();
    handle_relation(RelationCommand {
        common_options: config,
        csv: "ok.csv".to_string(),
        symbol_csv: "ok1.csv".to_string(),
        index_file: "".to_string(),
    }).unwrap();
}

#[test]
fn relation_v2_test() {
    let mut config = CommonOptions::default();
    config.exclude_file_regex = Some("".parse().unwrap());
    config.project_path = ".".parse().unwrap();
    handle_relation_v2(RelationCommand {
        common_options: config,
        csv: "".to_string(),
        symbol_csv: "".to_string(),
        index_file: "hello.index".to_string(),
    }).unwrap();
}
