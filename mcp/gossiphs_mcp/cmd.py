import fire
from gossiphs_mcp.server import app


class CLI:
    """Gossiphs MCP CLI"""

    def server(self, transport: str = "stdio"):
        app.run(transport)


def main():
    fire.Fire(CLI)


if __name__ == "__main__":
    main()
