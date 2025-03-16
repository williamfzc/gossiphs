import os
from mcp.server.fastmcp import FastMCP

from gossiphs_mcp.tools import file_impact

# Initialize FastMCP instance
mcp = FastMCP("Gossiphs Code Analysis")


@mcp.tool()
def analyze_file_impact(project_path: str, target_file: str) -> dict:
    """Analyze the impact scope of a specified file

    Args:
        project_path: Project root directory path
        target_file: Target file path

    Returns:
        dict: Dictionary containing analysis status and results
    """
    try:
        result = file_impact(project_path, target_file)
        return {"status": "success", "data": result, "message": ""}
    except Exception as e:
        return {"status": "error", "data": None, "message": str(e)}


def main():
    transport = os.environ.get("MCP_TRANSPORT", "sse")
    mcp.run(transport)


if __name__ == "__main__":
    main()
