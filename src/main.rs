use clap::Parser;
use git2::build::CheckoutBuilder;
use git2::{DiffOptions, Repository, RepositoryState};
use gossiphs::graph::{Graph, GraphConfig, RelatedFileContext};
use gossiphs::server::{server_main, ServerConfig};
use inquire::Text;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tracing::{debug, info};

#[derive(Parser, Debug)]
#[clap(
    name = "gossiphs",
    bin_name = "gossiphs",
    about = "gossiphs command line tool"
)]
struct Cli {
    #[clap(subcommand)]
    cmd: SubCommand,
}

#[derive(Parser, Debug)]
enum SubCommand {
    #[clap(name = "relate")]
    Relate(RelateCommand),

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

    #[clap(long)]
    #[clap(default_value = "gossiphs-diff.json")]
    json: String,
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
        return self.file.split(';').map(|each| each.to_string()).collect();
    }
}

fn main() {
    let cli: Cli = Cli::parse();

    match cli.cmd {
        SubCommand::Relate(search_cmd) => handle_relate(search_cmd),
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

    let g = Graph::from(config);

    let mut related_files_data = Vec::new();
    let files = relate_cmd.get_files();
    for file in &files {
        let mut files = g.related_files(&String::from(file));
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

fn handle_interactive(interactive_cmd: InteractiveCommand) {
    let mut config = GraphConfig::default();
    config.project_path = interactive_cmd.common_options.project_path.clone();
    if interactive_cmd.common_options.strict {
        config.def_limit = 1
    }

    let g = Graph::from(config);

    if interactive_cmd.dry {
        return;
    }

    loop {
        let file_path_result = Text::new("File Path:").prompt();
        match file_path_result {
            Ok(name) => {
                let files = g.related_files(&name);
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

    let g = Graph::from(config);

    // create mirror files
    // add links to files
    let files = g.files();
    match fs::create_dir(&obsidian_cmd.vault_dir) {
        Ok(_) => debug!("Directory created successfully."),
        Err(e) => panic!("Error creating directory: {}", e),
    }

    for each_file in files {
        let related = g.related_files(&each_file);
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
    name: String,
    added: Vec<RelatedFileContext>,
    removed: Vec<RelatedFileContext>,
}

fn handle_diff(diff_cmd: DiffCommand) {
    // repo status check
    let project_path = diff_cmd.common_options.project_path;
    let repo = Repository::open(&project_path).unwrap();
    if repo.state() != RepositoryState::Clean {
        println!("Working directory is dirty. Commit or stash changes first.");
        return;
    }
    let target_object = repo.revparse_single(&diff_cmd.target).unwrap();
    let target_commit = target_object.as_commit().unwrap();
    let source_object = repo.revparse_single(&diff_cmd.source).unwrap();
    let source_commit = source_object.as_commit().unwrap();

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
    let target_graph = Graph::from(config.clone());

    repo.checkout_tree(&source_object, Some(&mut builder))
        .unwrap();
    repo.set_head_detached(source_commit.id()).unwrap();
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
            .related_files(&each_file)
            .into_iter()
            .map(|item| return (item.name.clone(), item))
            .collect();
        let source_related_map: HashMap<String, RelatedFileContext> = source_graph
            .related_files(&each_file)
            .into_iter()
            .map(|item| return (item.name.clone(), item))
            .collect();
        let mut added_links: Vec<RelatedFileContext> = Vec::new();
        for (_, item) in source_related_map.clone() {
            if !target_related_map.contains_key(&item.name) {
                added_links.push(item);
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
            removed: removed_links,
        })
    }

    let json = serde_json::to_string(&ret).unwrap();
    fs::write(diff_cmd.json, json).expect("");
}

#[test]
fn test_handle_relate() {
    let relate_cmd = RelateCommand {
        common_options: CommonOptions {
            project_path: String::from("."),
            strict: false,
        },
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
        common_options: CommonOptions {
            project_path: String::from("."),
            strict: false,
        },
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
        common_options: CommonOptions {
            project_path: String::from("."),
            strict: true,
        },
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
        common_options: CommonOptions {
            project_path: String::from("."),
            strict: false,
        },
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
        common_options: CommonOptions {
            project_path: ".".to_string(),
            strict: false,
        },
        port: 9411,
    })
}

#[test]
#[ignore]
fn obsidian_test() {
    handle_obsidian(ObsidianCommand {
        common_options: CommonOptions {
            project_path: ".".to_string(),
            strict: false,
        },
        vault_dir: "./vault".to_string(),
    })
}

#[test]
fn diff_test() {
    handle_diff(DiffCommand {
        common_options: CommonOptions {
            project_path: ".".parse().unwrap(),
            strict: false,
        },
        target: "HEAD~10".to_string(),
        source: "HEAD".to_string(),
        json: "gossiphs.json".to_string(),
    })
}
