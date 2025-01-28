import networkx as nx
from gossiphs import GraphConfig, create_graph, Graph

# Create the graph using gossiphs
config = GraphConfig()
config.project_path = "../.."
graph: Graph = create_graph(config)

# Create a NetworkX directed graph
nx_graph = nx.DiGraph()

# Add nodes and edges to the NetworkX graph
for each_file in graph.files():
    nx_graph.add_node(each_file, metadata=graph.file_metadata(each_file))

    related_files = graph.related_files(each_file)
    for each_related_file in related_files:
        related_symbols = set(each_symbol.symbol.name for each_symbol in each_related_file.related_symbols)

        # use symbol count as weight
        nx_graph.add_edge(
            each_file,
            each_related_file.name,
            weight=len(list(related_symbols)),
        )

print(f"NetworkX graph created with {nx_graph.number_of_nodes()} nodes and {nx_graph.number_of_edges()} edges.")

# Compute PageRank
pagerank_scores = nx.pagerank(nx_graph)

# Print PageRank scores
print("\nPageRank Scores:")
for node, score in sorted(pagerank_scores.items()):
    print(f"{node}: {score:.4f}")
