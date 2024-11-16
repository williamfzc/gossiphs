use gossiphs::graph::{Graph, GraphConfig};
use gossiphs::symbol::SymbolKind;

fn main() {
    let config = GraphConfig::default();
    let g = Graph::from(config);

    // done! just try it
    let all_files = g.files();
    for file in &all_files {
        // related file search
        let related_files = g.related_files(file.clone());
        for each_related in &related_files {
            println!("{} -> {}: {}", file, each_related.name, each_related.score);
        }

        // file details
        if !related_files.is_empty() {
            let random_file = related_files[0].name.clone();
            let meta = g.file_metadata(random_file.clone());
            println!("symbols in {}: {:?}", random_file, meta.symbols.len());

            // search all the references of symbols from this file
            for each_symbol in &meta.symbols {
                if each_symbol.kind != SymbolKind::DEF {
                    continue;
                }

                for (each_related_symbol, each_score) in g.related_symbols(each_symbol.clone()) {
                    if each_score == 0 {
                        continue;
                    }
                    if each_related_symbol.file == *file {
                        continue;
                    }

                    println!(
                        "{}: DEF {} line#{} -> REF {} line#{}",
                        each_symbol.name,
                        random_file,
                        each_symbol.range.start_point.row,
                        each_related_symbol.file,
                        each_related_symbol.range.start_point.row
                    )
                }
            }
        }
    }
}
