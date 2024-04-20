use gossiphs::graph::{Graph, GraphConfig};

fn main() {
    let config = GraphConfig::default();
    let g = Graph::from(config);

    // done! just try it
    let all_files = g.files();
    for file in &all_files {
        // related file search
        let related_files = g.related_files(file);
        for each_related in &related_files {
            println!("{} -> {}: {}", file, each_related.name, each_related.score);
        }

        // file details
        if !related_files.is_empty() {
            let random_file = related_files[0].name.clone();
            let meta = g.file_metadata(&random_file);
            println!("symbols in {}: {:?}", random_file, meta.symbols.len());

            // and query the symbol infos
        }
    }
}
