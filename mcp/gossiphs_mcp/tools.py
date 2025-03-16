from collections import OrderedDict
from gossiphs import GraphConfig, create_graph

# Cache for project dependency graph instances with max size of 100
_project_graph_cache = OrderedDict()
MAX_CACHE_SIZE = 10


def file_impact(project_path: str, target_file: str):
    """Analyze the impact scope of a file

    Args:
        project_path: Project root directory path
        target_file: Target file path

    Returns:
        list: List of files related to the target file
    """
    # Get or create project dependency graph
    if project_path not in _project_graph_cache:
        if len(_project_graph_cache) >= MAX_CACHE_SIZE:
            # Remove the least recently used item when cache is full
            _project_graph_cache.popitem(last=False)

        config = GraphConfig()
        config.project_path = project_path
        _project_graph_cache[project_path] = create_graph(config)
    else:
        # Move accessed item to the end to mark it as most recently used
        _project_graph_cache.move_to_end(project_path)

    # Analyze using cached dependency graph
    graph = _project_graph_cache[project_path]
    related_files = graph.related_files(target_file)

    ret = []
    for each_related_file in related_files:
        related_symbols = set(
            each_symbol.symbol.name for each_symbol in each_related_file.related_symbols
        )
        ret.append(
            {
                "file": each_related_file.name,
                "symbols": list(related_symbols),
            }
        )

    return ret
