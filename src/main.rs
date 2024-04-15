use clap::Parser;
use gossiphs::graph::{Graph, GraphConfig};

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
}

#[derive(Parser, Debug)]
struct RelateCommand {
    #[clap(long)]
    project_path: String,

    #[clap(long)]
    file: String,
}

fn main() {
    let cli: Cli = Cli::parse();

    match cli.cmd {
        SubCommand::Relate(search_cmd) => handle_relate(search_cmd),
    }
}

fn handle_relate(relate_cmd: RelateCommand) {
    let mut config = GraphConfig::default();
    config.project_path = relate_cmd.project_path;
    let g = Graph::from(config);
    let files = g.related_files(&relate_cmd.file);
    // convert to JSON and print to stdout
    let json = serde_json::to_string_pretty(&files).unwrap();
    println!("{}", json);
}

#[test]
fn test_handle_relate() {
    let relate_cmd = RelateCommand {
        project_path: String::from("."),
        file: "src/extractor.rs".to_string(),
    };
    handle_relate(relate_cmd);
}
