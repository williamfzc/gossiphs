## Usage

### As a command line tool

You can find pre-compiled files for your platform
on [Our Release Page](https://github.com/williamfzc/gossiphs/releases). After extraction, you can use `gossiphs --help`
to find the corresponding help.

#### (👍Recommended) Export file relation matrix to csv

```bash
gossiphs relation
gossiphs relation --csv scores.csv --symbol-csv symbols.csv
```

And you can use something like [pandas](https://pandas.pydata.org/) to handle this matrix and apply further analysis
without accessing the rust part.

##### scores.csv

shows the relations between files by int score.

|                  | examples/mini.rs | src/extractor.rs | src/graph.rs | src/lib.rs | src/main.rs | src/rule.rs | src/server.rs | src/symbol.rs |
|------------------|------------------|------------------|--------------|------------|-------------|-------------|---------------|---------------|
| examples/mini.rs |                  |                  |              |            |             |             |               |               |
| src/extractor.rs |                  |                  | 8            |            |             |             | 1             |               |
| src/graph.rs     | 9                |                  |              |            | 23          |             | 5             |               |
| src/lib.rs       |                  |                  |              |            |             |             |               |               |
| src/main.rs      |                  |                  | 5            |            |             |             | 1             |               |
| src/rule.rs      |                  | 18               |              |            |             |             |               |               |
| src/server.rs    |                  |                  |              |            | 2           |             |               |               |
| src/symbol.rs    | 1                | 28               | 64           |            | 32          |             | 13            |

- By Column: `src/graph.rs` and `src/symbol.rs` have been used by `example/mini.rs`.
- By Row: `src/rule.rs` has only been used by `src/extractor.rs`.

##### **symbols.csv**

shows the relations between files by real reference names.

|                  | examples/mini.rs                                              | src/extractor.rs                | src/graph.rs                                                                                                                                                                                                                                              | src/lib.rs                         | src/main.rs                                                                             | src/rule.rs | src/server.rs | src/symbol.rs |
|------------------|---------------------------------------------------------------|---------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------------------------------|-----------------------------------------------------------------------------------------|-------------|---------------|---------------|
| examples/mini.rs |                                                               |                                 |                                                                                                                                                                                                                                                           |                                    |                                                                                         |             |               |               |
| src/extractor.rs |                                                               |                                 | extract                                                                                                                                                                                                                                                   |                                    |                                                                                         |             | extract       |               |
| src/graph.rs     | file_metadata\|default\|related_files\|related_symbols\|files |                                 | related_files\|files                                                                                                                                                                                                                                      |                                    | file_metadata\|files\|empty\|related_files                                              |             |               |               |
| src/lib.rs       |                                                               |                                 |                                                                                                                                                                                                                                                           |                                    |                                                                                         |             |               |               |
| src/main.rs      |                                                               | default                         |                                                                                                                                                                                                                                                           |                                    | main                                                                                    |             |               |               |
| src/rule.rs      |                                                               | get_rule                        |                                                                                                                                                                                                                                                           |                                    |                                                                                         |             |               |               |
| src/server.rs    |                                                               |                                 |                                                                                                                                                                                                                                                           |                                    |                                                                                         | server_main |               |               |
| src/symbol.rs    | from                                                          | new\|id\|new_ref\|from\|new_def | list_definitions\|list_symbols\|id\|link_symbol_to_symbol\|link_file_to_symbol\|list_references_by_definition\|enhance_symbol_to_symbol\|get_symbol\|from\|new\|add_symbol\|add_file\|list_references\|pairs_between_files\|list_definitions_by_reference | new\|id\|from\|pairs_between_files | list_references_by_definition\|new\|from\|list_definitions_by_reference\|get_symbol\|id |

- By column: **example/mini.rs** using `file_metadata`/`related_files` ... from `src/graph.rs`.

<details><summary>Other functions ...</summary>

#### Diff with context

```bash
# diff between HEAD and HEAD~1
gossiphs diff

# custom diff
gossiphs diff --target HEAD~5
gossiphs diff --target d18a5db39752d244664a23f74e174448b66b5b7e

# output json
gossiphs diff --json
```

output:

```text
src/services/user-info/index.ts
├── src/background-script/driveUploader.ts (ADDED)
├── src/background-script/task.ts (DELETED)
├── scripts/download-config.js (DELETED)
├── src/background-script/sdk.ts
├── src/services/user-info/listener.ts
├── src/services/config/index.ts
├── src/content-script/modal.ts
├── src/background-script/help-center.ts
```

- ADDED: Refers to file relationships added in this diff
- DELETED: Refers to file relationships deleted in this diff
- Others: Refers to file relationships that were not affected by this diff and originally existed

#### Obsidian Graph

For example, you can use this command to generate
an [obsidian vault](https://help.obsidian.md/Getting+started/Create+a+vault):

```bash
gossiphs obsidian --project-path . --vault-dir ./target_vault
```

and get a code relation graph:

<img width="644" alt="image" src="https://github.com/williamfzc/gossiphs/assets/13421694/03a35063-56b4-4d23-8a24-612708030138">

</details>

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

### As a local server

Starting a local server similar to LSP for other clients to use may be a reasonable approach, which is what we are
currently doing.

```bash
./gossiphs server --project-path ./your/project --strict
```

API desc can be found [here](../src/server.rs).
