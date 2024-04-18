use clap::Parser;
use gossiphs::graph::{Graph, GraphConfig};
use inquire::Text;
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
    file: String,

    #[clap(long)]
    #[clap(default_value = None)]
    json: Option<String>,
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
    config.project_path = relate_cmd.project_path;
    let g = Graph::from(config);
    let files = g.related_files(&relate_cmd.file);

    let json = serde_json::to_string(&files).unwrap();
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
                let json = serde_json::to_string_pretty(&files).unwrap();
                println!("{}", json);
            }
            Err(_) => break,
        }
    }
}

#[test]
fn test_handle_relate() {
    let relate_cmd = RelateCommand {
        project_path: String::from("."),
        file: "src/extractor.rs".to_string(),
        json: None,
    };
    handle_relate(relate_cmd);
}
