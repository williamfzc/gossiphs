# Gossiphs = Gossip Graphs

![Crates.io Version](https://img.shields.io/crates/v/gossiphs)
[![RealWorld Test](https://github.com/williamfzc/gossiphs/actions/workflows/cargo-test.yml/badge.svg)](https://github.com/williamfzc/gossiphs/actions/workflows/cargo-test.yml)

An experimental Rust library for general code file relationship analysis. Based on tree-sitter and git analysis.

## Goal & Motivation

Code navigation is a fascinating subject that plays a pivotal role in various domains, such as:

- Guiding the context during the development process within an IDE.
- Facilitating more convenient code browsing on websites.
- Analyzing the impact of code changes in Continuous Integration (CI) systems.
- ...

In the past, I endeavored to apply [LSP/LSIF technologies](https://lsif.dev/) and techniques
like [Github's Stack-Graphs](https://dcreager.net/talks/stack-graphs/) to impact analysis, encountering different
challenges along the way. For our needs, a method akin to Stack-Graphs aligns most closely with our expectations.
However, the challenges are evident: it requires crafting highly language-specific rules, which is a considerable
investment for us, given that we do not require such high precision data.

We attempt to make some trade-offs on the challenges currently faced by
stack-graphs to achieve our expected goals to a certain extent:

- Zero repo-specific configuration: It can be applied to most languages and repositories without additional
  configuration.
- Low extension cost: adding rules for languages is not high.
- Acceptable precision: We have sacrificed a certain level of precision, but we also hope that it remains at an
  acceptable level.

## How it works

Gossiphs constructs a graph that interconnects symbols of definitions and references.

1. Extract imports and exports: Identify the imports and exports of each file.
2. Connect nodes: Establish connections between potential definition and reference nodes.
3. Refine edges with commit histories: Utilize commit histories to refine the relationships between nodes.

Unlike stack-graphs, we have omitted the highly complex scope analysis and instead opted to refine our edges using
commit histories.
This approach significantly reduces the complexity of rule writing, as the rules only need to specify which types of
symbols should be exported or imported for each file.

While there is undoubtedly a trade-off in precision, the benefits are clear:

1. Minimal impact on accuracy: In practical scenarios, the loss of precision is not as significant as one might expect.
2. Commit history relevance: The use of commit history to reflect the influence between code segments aligns well with
   our objectives.
3. Language support: We can easily support the vast majority of programming languages, meeting the analysis needs of
   various types of repositories.

## Supported Languages

We are expanding language support based on [Tree-Sitter Query](https://tree-sitter.github.io/tree-sitter/code-navigation-systems), which isn't too costly. 
If you're interested, you can check out the [contribution](#contribution) section.

| Language   | Status |
|------------|--------|
| Rust       | ✅      |
| Python     | ✅      |
| TypeScript | ✅      |
| Golang     | ✅      |

## Usage

The project is still in the experimental stage.

### As a command line tool

You can find pre-compiled files for your platform
on [Our Release Page](https://github.com/williamfzc/gossiphs/releases). After extraction, you can use `gossiphs --help`
to find the corresponding help.

For example, you can use this command to generate
an [obsidian vault](https://help.obsidian.md/Getting+started/Create+a+vault):

```bash
gossiphs obsidian --project-path . --vault-dir ./target_vault
```

and get a code relation graph:

<img width="644" alt="image" src="https://github.com/williamfzc/gossiphs/assets/13421694/03a35063-56b4-4d23-8a24-612708030138">

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

API desc can be found [here](./src/server.rs).

## Precision

The method we use to demonstrate accuracy is to compare the results with those of LSP/LSIF. It must be admitted that
static inference is almost impossible to obtain all reference relationships like LSP, but in strict mode, our
calculation accuracy is still quite considerable. In normal mode, you can decide whether to adopt the relationship based
on the weight returned.

| Repo                                | Precision (Strict Mode) | Graph Generated Time |
|-------------------------------------|-------------------------|----------------------|
| https://github.com/williamfzc/srctx | 80/80 = 100 %           | 83.139791ms          |
| https://github.com/gin-gonic/gin    | 160/167 = 95.80838 %    | 310.6805ms           |

## Contribution

The project is still in a very early and experimental stage. If you are interested, please leave your thoughts through
an issue. In the short term, we hope to build better support for more languages.

You just need to:

1. Edit rules in [src/rule.rs](src/rule.rs)
2. Test it in [src/extractor.rs](src/extractor.rs)
3. Try it with your repo in [src/graph.rs](src/graph.rs)

## License

[Apache 2.0](LICENSE)
