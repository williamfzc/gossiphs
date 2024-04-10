use clap::Parser;
use gossiphs::graph::{Graph, GraphConfig};
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
    #[clap(name = "search")]
    Search(SearchCommand),
}

#[derive(Parser, Debug)]
struct SearchCommand {
    #[clap(long)]
    symbol_name: String,
}

fn main() {
    let cli: Cli = Cli::parse();

    match cli.cmd {
        SubCommand::Search(search_cmd) => handle_search(search_cmd),
    }
}

fn handle_search(search_cmd: SearchCommand) {
    tracing_subscriber::fmt::init();
    info!(search_cmd.symbol_name);
    Graph::from(GraphConfig::default());
}
