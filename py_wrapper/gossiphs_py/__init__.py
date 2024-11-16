import gossiphs

config = gossiphs.GraphConfig()
config.project_path = "../.."
graph = gossiphs.create_graph(config)
print(graph)
files = graph.files()
print(files)

for each in files:
    f = graph.related_files(each)
    print(f)
