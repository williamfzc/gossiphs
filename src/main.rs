use clap::Parser;
use csv::Writer;
use git2::build::CheckoutBuilder;
use git2::{Commit, DiffOptions, Error, Object, ObjectType, Repository, Status};
use gossiphs::server::{server_main, ServerConfig};
use indicatif::ProgressBar;
use inquire::Text;
use rayon::iter::ParallelIterator;
use rayon::prelude::IntoParallelRefIterator;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use termtree::Tree;
use tracing::{debug, info};
use gossiphs::api::RelatedFileContext;
use gossiphs::graph::{Graph, GraphConfig};

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

    #[clap(name = "interactive")]
    Interactive(InteractiveCommand),

    #[clap(name = "server")]
    Server(ServerCommand),

    #[clap(name = "obsidian")]
    Obsidian(ObsidianCommand),

    /// Diff analysis (will do some real checkout)
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

    /// git commit history search depth
    #[clap(long)]
    depth: Option<u32>,

    #[clap(long)]
    exclude_file_regex: Option<String>,

    #[clap(long)]
    exclude_author_regex: Option<String>,
}

impl CommonOptions {
    #[cfg(test)]
    fn default() -> CommonOptions {
        CommonOptions {
            project_path: String::from("."),
            strict: false,
            depth: None,
            exclude_file_regex: None,
            exclude_author_regex: None,
        }
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

fn main() {
    let cli: Cli = Cli::parse();

    match cli.cmd {
        SubCommand::Relate(search_cmd) => handle_relate(search_cmd),
        SubCommand::Relation(relation_cmd) => handle_relation(relation_cmd),
        SubCommand::Interactive(interactive_cmd) => handle_interactive(interactive_cmd),
        SubCommand::Server(server_cmd) => handle_server(server_cmd),
        SubCommand::Obsidian(obsidian_cmd) => handle_obsidian(obsidian_cmd),
        SubCommand::Diff(diff_cmd) => handle_diff(diff_cmd),
    }
}

fn handle_relate(relate_cmd: RelateCommand) {
    // result will be saved to file, so enable log
    if !relate_cmd.json.is_none() {
        tracing_subscriber::fmt::init();
    }
    let mut config = GraphConfig::default();
    config.project_path = relate_cmd.common_options.project_path.clone();
    if relate_cmd.common_options.strict {
        config.def_limit = 1
    }
    if !relate_cmd.common_options.depth.is_none() {
        config.depth = relate_cmd.common_options.depth.unwrap();
    }

    let g = Graph::from(config);

    let mut related_files_data = Vec::new();
    let files = relate_cmd.get_files();
    for file in &files {
        let mut files = g.related_files(String::from(file));
        if relate_cmd.ignore_zero {
            files.retain(|each| each.score > 0);
        }
        related_files_data.push(RelatedFileWrapper {
            name: file.to_string(),
            related: files,
        });
    }
    let json = serde_json::to_string(&related_files_data).unwrap();
    if !relate_cmd.json.is_none() {
        fs::write(relate_cmd.json.unwrap(), json).expect("");
    } else {
        println!("{}", json);
    }
}

fn handle_relation(relation_cmd: RelationCommand) {
    let mut config = GraphConfig::default();
    config.project_path = relation_cmd.common_options.project_path.clone();
    if relation_cmd.common_options.strict {
        config.def_limit = 1;
    }
    if let Some(depth) = relation_cmd.common_options.depth {
        config.depth = depth;
    }
    if let Some(exclude) = relation_cmd.common_options.exclude_file_regex {
        config.exclude_file_regex = exclude;
    }
    config.exclude_author_regex = relation_cmd.common_options.exclude_author_regex.clone();

    let g = Graph::from(config);

    let mut files: Vec<String> = g.files().into_iter().collect();
    files.sort();

    // Create a new CSV writer
    let wtr_result = Writer::from_path(relation_cmd.csv);
    let mut wtr = match wtr_result {
        Ok(writer) => writer,
        Err(e) => panic!("Failed to create CSV writer: {}", e),
    };
    // Write the header row
    let mut header = vec!["".to_string()];
    header.extend(files.clone());
    if let Err(e) = wtr.write_record(&header) {
        panic!("Failed to write CSV header: {}", e);
    }

    let mut symbol_wtr_opts = None;
    if !relation_cmd.symbol_csv.is_empty() {
        let symbol_wtr_result = Writer::from_path(relation_cmd.symbol_csv);
        symbol_wtr_opts = match symbol_wtr_result {
            Ok(writer) => Some(writer),
            Err(e) => panic!("Failed to create CSV writer: {}", e),
        };
        let mut header = vec!["".to_string()];
        header.extend(files.clone());
        if let Some(symbol_wtr) = symbol_wtr_opts.as_mut() {
            symbol_wtr
                .write_record(&header)
                .expect("Failed to write header to symbol_wtr");
        }
    }

    // Write each row
    let pb = ProgressBar::new(files.len() as u64);
    let results: Vec<(Vec<String>, Vec<String>)> = files
        .par_iter()
        .map(|file| {
            pb.inc(1);
            let mut row = vec![file.clone()];
            let mut pair_row = vec![file.clone()];
            let related_files_map: HashMap<_, _> = g
                .related_files(file.clone())
                .into_iter()
                .map(|rf| (rf.name, rf.score))
                .collect();

            for related_file in &files {
                if let Some(score) = related_files_map.get(related_file) {
                    if *score > 0 {
                        row.push(score.to_string());
                        if symbol_wtr_opts.is_some() {
                            let pairs = g
                                .pairs_between_files(file.clone(), related_file.clone())
                                .iter()
                                .map(|each| each.src_symbol.name.clone())
                                .collect::<Vec<String>>();
                            pair_row.push(pairs.join("|"));
                        }
                    } else {
                        row.push(String::new());
                        pair_row.push(String::new());
                    }
                } else {
                    row.push(String::new());
                    pair_row.push(String::new());
                }
            }

            (row, pair_row)
        })
        .collect();
    pb.finish_and_clear();

    for (row, pair_row) in results {
        wtr.write_record(&row).expect("Failed to write record");
        if let Some(symbol_wtr) = symbol_wtr_opts.as_mut() {
            symbol_wtr
                .write_record(&pair_row)
                .expect("Failed to write pair_row to symbol_wtr");
        }
    }

    // Flush the writer to ensure all data is written
    if let Err(e) = wtr.flush() {
        panic!("Failed to flush CSV writer: {}", e);
    }
}

fn handle_interactive(interactive_cmd: InteractiveCommand) {
    let mut config = GraphConfig::default();
    config.project_path = interactive_cmd.common_options.project_path.clone();
    if interactive_cmd.common_options.strict {
        config.def_limit = 1
    }
    if !interactive_cmd.common_options.depth.is_none() {
        config.depth = interactive_cmd.common_options.depth.unwrap();
    }

    let g = Graph::from(config);

    if interactive_cmd.dry {
        return;
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
                .unwrap();
                println!("{}", json);
            }
            Err(_) => break,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RelatedFileWrapper {
    pub name: String,
    pub related: Vec<RelatedFileContext>,
}

fn handle_server(server_cmd: ServerCommand) {
    tracing_subscriber::fmt::init();
    let mut config = GraphConfig::default();
    config.project_path = server_cmd.common_options.project_path.clone();
    if server_cmd.common_options.strict {
        config.def_limit = 1
    }
    if !server_cmd.common_options.depth.is_none() {
        config.depth = server_cmd.common_options.depth.unwrap();
    }

    let g = Graph::from(config);

    let mut server_config = ServerConfig::new(g);
    server_config.port = server_cmd.port.clone();
    info!("server up, port: {}", server_config.port);
    server_main(server_config);
}

fn handle_obsidian(obsidian_cmd: ObsidianCommand) {
    tracing_subscriber::fmt::init();
    let mut config = GraphConfig::default();
    config.project_path = obsidian_cmd.common_options.project_path.clone();
    if obsidian_cmd.common_options.strict {
        config.def_limit = 1
    }
    if !obsidian_cmd.common_options.depth.is_none() {
        config.depth = obsidian_cmd.common_options.depth.unwrap();
    }

    let g = Graph::from(config);

    // create mirror files
    // add links to files
    let files = g.files();
    match fs::create_dir(&obsidian_cmd.vault_dir) {
        Ok(_) => debug!("Directory created successfully."),
        Err(e) => panic!("Error creating directory: {}", e),
    }

    for each_file in files {
        let related = g.related_files(each_file.clone());
        let markdown_filename = format!("{}/{}.md", &obsidian_cmd.vault_dir, each_file);
        let mut markdown_content = String::new();
        for related_file in related {
            markdown_content.push_str(&format!("[[{}]]\n", related_file.name));
        }

        let path = Path::new(&markdown_filename);
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        if let Err(why) = fs::create_dir_all(parent) {
            panic!("couldn't create directory {}: {}", parent.display(), why);
        }
        let mut file = match File::create(&markdown_filename) {
            Err(why) => panic!("couldn't create {}: {}", markdown_filename, why),
            Ok(file) => file,
        };
        match file.write_all(markdown_content.as_bytes()) {
            Err(why) => panic!("couldn't write to {}: {}", markdown_filename, why),
            Ok(_) => debug!("Successfully wrote to {}", markdown_filename),
        }
    }
}
#[derive(Serialize, Deserialize)]
struct DiffFileContext {
    // same as git
    name: String,
    added: Vec<RelatedFileContext>,
    deleted: Vec<RelatedFileContext>,
    modified: Vec<RelatedFileContext>,
}

fn is_working_directory_clean(repo: &Repository) -> bool {
    match repo.statuses(None) {
        Ok(statuses) => {
            for entry in statuses.iter() {
                let status = entry.status();
                if status.contains(Status::WT_NEW)
                    || status.contains(Status::WT_MODIFIED)
                    || status.contains(Status::WT_DELETED)
                    || status.contains(Status::WT_TYPECHANGE)
                    || status.contains(Status::WT_RENAMED)
                    || status.contains(Status::INDEX_NEW)
                    || status.contains(Status::INDEX_MODIFIED)
                    || status.contains(Status::INDEX_DELETED)
                    || status.contains(Status::INDEX_TYPECHANGE)
                    || status.contains(Status::INDEX_RENAMED)
                {
                    return false;
                }
            }
            true
        }
        Err(_) => false,
    }
}

fn get_current_branch(repo: &Repository) -> Option<String> {
    let head = repo.head().ok()?;
    let shorthand = head.shorthand()?;
    Some(shorthand.to_string())
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

fn handle_diff(diff_cmd: DiffCommand) {
    // repo status check
    let project_path = diff_cmd.common_options.project_path;
    let repo = Repository::open(&project_path).unwrap();
    if !is_working_directory_clean(&repo) {
        println!("Working directory is dirty. Commit or stash changes first.");
        return;
    }
    let current_branch = get_current_branch(&repo);
    let (target_commit, target_object) = get_commit_and_object(&repo, &diff_cmd.target).unwrap();
    let (source_commit, source_object) = get_commit_and_object(&repo, &diff_cmd.source).unwrap();

    // gen graphs
    let mut builder = CheckoutBuilder::new();
    builder.force();
    repo.checkout_tree(&target_object, Some(&mut builder))
        .unwrap();
    repo.set_head_detached(target_commit.id()).unwrap();

    let mut config = GraphConfig::default();
    config.project_path = project_path;
    if diff_cmd.common_options.strict {
        config.def_limit = 1
    }
    if !diff_cmd.common_options.depth.is_none() {
        config.depth = diff_cmd.common_options.depth.unwrap();
    }

    let target_graph = Graph::from(config.clone());

    repo.checkout_tree(&source_object, Some(&mut builder))
        .unwrap();
    repo.set_head_detached(source_commit.id()).unwrap();
    // reset to branch
    if !current_branch.is_none() {
        let current_branch_str = current_branch.unwrap();
        if let Err(e) = repo.set_head(&format!("refs/heads/{}", current_branch_str)) {
            eprintln!(
                "Failed to switch back to branch '{}': {}",
                current_branch_str, e
            );
        }
    }

    let source_graph = Graph::from(config);

    // diff files
    let mut diff_options = DiffOptions::new();
    let diff = repo
        .diff_tree_to_tree(
            Some(&target_commit.tree().unwrap()),
            Some(&source_commit.tree().unwrap()),
            Some(&mut diff_options),
        )
        .unwrap();

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
    .unwrap();

    // diff context
    let mut ret: Vec<DiffFileContext> = Vec::new();
    for each_file in diff_files {
        let target_related_map: HashMap<String, RelatedFileContext> = target_graph
            .related_files(each_file.clone())
            .into_iter()
            .map(|item| return (item.name.clone(), item))
            .collect();
        let source_related_map: HashMap<String, RelatedFileContext> = source_graph
            .related_files(each_file.clone())
            .into_iter()
            .map(|item| return (item.name.clone(), item))
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
        let json = serde_json::to_string(&ret).unwrap();
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
    handle_relate(relate_cmd);
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
    handle_relate(relate_cmd);
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
    handle_relate(relate_cmd);
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
    handle_relate(relate_cmd);
}

#[test]
#[ignore]
fn server_test() {
    handle_server(ServerCommand {
        common_options: CommonOptions::default(),
        port: 9411,
    })
}

#[test]
#[ignore]
fn obsidian_test() {
    handle_obsidian(ObsidianCommand {
        common_options: CommonOptions::default(),
        vault_dir: "./vault".to_string(),
    })
}

#[test]
fn diff_test() {
    handle_diff(DiffCommand {
        common_options: CommonOptions::default(),
        target: "HEAD~10".to_string(),
        source: "HEAD".to_string(),
        json: false,
    });

    handle_diff(DiffCommand {
        common_options: CommonOptions::default(),
        target: "d18a5db39752d244664a23f74e174448b66b5b7e".to_string(),
        source: "HEAD".to_string(),
        json: false,
    });
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
    })
}
