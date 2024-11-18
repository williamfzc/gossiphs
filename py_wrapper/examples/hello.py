from gossiphs import GraphConfig, create_graph

config = GraphConfig()
config.project_path = "../.."
graph = create_graph(config)
print(graph)
files = graph.files()
print(files)

for each in files:
    metadata = graph.file_metadata(each)
    for each_symbol in metadata.symbols:
        if each_symbol.is_def():
            print(each_symbol.name)

    related_files = graph.related_files(each)
    for each_related_file in related_files:
        print(f"{each} -> {each_related_file.name}")

        symbols = set((each_symbol.symbol.name for each_symbol in each_related_file.related_symbols))
        print(symbols)
