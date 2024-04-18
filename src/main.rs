use clap::Parser;
use gossiphs::graph::{Graph, GraphConfig, RelatedFileContext};
use inquire::Text;
use serde::{Deserialize, Serialize};
use std::fs;

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
}

#[derive(Parser, Debug)]
struct RelateCommand {
    #[clap(long)]
    #[clap(default_value = ".")]
    project_path: String,

    #[clap(long)]
    #[clap(default_value = "")]
    file: String,

    #[clap(long)]
    #[clap(default_value = "")]
    file_txt: String,

    #[clap(long)]
    #[clap(default_value = None)]
    json: Option<String>,
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

#[derive(Parser, Debug)]
struct InteractiveCommand {
    #[clap(long)]
    #[clap(default_value = ".")]
    project_path: String,
}

fn main() {
    let cli: Cli = Cli::parse();

    match cli.cmd {
        SubCommand::Relate(search_cmd) => handle_relate(search_cmd),
        SubCommand::Interactive(interactive_cmd) => handle_interactive(interactive_cmd),
    }
}

fn handle_relate(relate_cmd: RelateCommand) {
    // result will be saved to file, so enable log
    if !relate_cmd.json.is_none() {
        tracing_subscriber::fmt::init();
    }
    let mut config = GraphConfig::default();
    config.project_path = relate_cmd.project_path.clone();
    let g = Graph::from(config);

    let mut related_files_data = Vec::new();
    let files = relate_cmd.get_files();
    for file in &files {
        let files = g.related_files(&String::from(file));
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
    config.project_path = interactive_cmd.project_path;
    let g = Graph::from(config);

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

#[test]
fn test_handle_relate() {
    let relate_cmd = RelateCommand {
        project_path: String::from("."),
        file: "src/extractor.rs".to_string(),
        file_txt: "".to_string(),
        json: None,
    };
    handle_relate(relate_cmd);
}

#[test]
fn test_handle_relate_files() {
    let relate_cmd = RelateCommand {
        project_path: String::from("."),
        file: "src/extractor.rs;src/main.rs;src/graph.rs".to_string(),
        file_txt: "".to_string(),
        json: None,
    };
    handle_relate(relate_cmd);
}

#[test]
#[ignore]
fn test_handle_relate_file_txt() {
    let relate_cmd = RelateCommand {
        project_path: String::from("."),
        file: "".to_string(),
        file_txt: "./aa.txt".to_string(),
        json: None,
    };
    handle_relate(relate_cmd);
}
