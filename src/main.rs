use clap::Parser;
use gossiphs::graph::{Graph, GraphConfig, RelatedFileContext};
use gossiphs::server::{server_main, ServerConfig};
use inquire::Text;
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::info;

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
