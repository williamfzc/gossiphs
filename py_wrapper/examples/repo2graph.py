import networkx as nx
from gossiphs import GraphConfig, create_graph, Graph

config = GraphConfig()
config.project_path = "../.."
graph: Graph = create_graph(config)

nx_graph = nx.DiGraph()

for each_file in graph.files():
    nx_graph.add_node(each_file, metadata=graph.file_metadata(each_file))

    related_files = graph.related_files(each_file)
    for each_related_file in related_files:
        related_symbols = set(each_symbol.symbol.name for each_symbol in each_related_file.related_symbols)

        nx_graph.add_edge(
            each_file,
            each_related_file.name,
            related_symbols=list(related_symbols)
        )

print(f"NetworkX graph created with {nx_graph.number_of_nodes()} nodes and {nx_graph.number_of_edges()} edges.")

for src, dest, data in nx_graph.edges(data=True):
    print(f"{src} -> {dest}, related symbols: {data['related_symbols']}")
