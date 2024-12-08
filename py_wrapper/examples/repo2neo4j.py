import networkx as nx
from neo4j import GraphDatabase

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

# upload to neo4j
uri = "bolt://localhost:7687"
username = "neo4j"
password = "williamfzc"
driver = GraphDatabase.driver(uri, auth=(username, password))


def execute_query(query, parameters=None):
    with driver.session() as session:
        session.run(query, parameters)


def upload_nodes(nx_graph):
    for node, attributes in nx_graph.nodes(data=True):
        query = """
        MERGE (n:File {name: $name}) 
        """
        execute_query(query, {"name": node})


def upload_edges(nx_graph):
    for source, target, attributes in nx_graph.edges(data=True):
        related_symbols = attributes.get('related_symbols', [])
        query = """
        MATCH (a:File {name: $source})
        MATCH (b:File {name: $target})
        MERGE (a)-[r:RELATES_TO]->(b)
        SET r.related_symbols = $related_symbols
        """
        execute_query(query, {"source": source, "target": target, "related_symbols": related_symbols})


upload_nodes(nx_graph)
upload_edges(nx_graph)

print("Graph uploaded to Neo4j.")
driver.close()
