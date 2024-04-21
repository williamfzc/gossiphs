# Gossiphs = Gossip Graphs

A Rust library for general code file relationship analysis. Based on tree-sitter and git analysis.

## Goal & Motivation

This repository is largely inspired by Github's Stack-Graphs.
We attempt to make some trade-offs on the challenges currently faced by
stack-graphs (https://dcreager.net/talks/stack-graphs/) to achieve our expected goals to a certain extent:

- It can be applied to most languages and repositories without additional configuration.
- The cost of writing rules for languages is not high.
- We have sacrificed a certain level of precision, but we also hope that it remains at an acceptable level.

## Usage

The project is still in the experimental stage.

### As a rust library

Please refer to [examples](examples) for usage.

```rust
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
```

### As a command line tool

You can find pre-compiled files for your platform
on [Our Release Page](https://github.com/williamfzc/gossiphs/releases). After extraction, you can use `gossiphs --help`
to find the corresponding help.

## License

[Apache 2.0](LICENSE)
