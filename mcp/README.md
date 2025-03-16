# gossiphs_mcp

[gossiphs](https://github.com/williamfzc/gossiphs) wrapper for Model Context Protocol (MCP)

## Goal

The goal is to expose gossiphs' code analysis capabilities in a simple way (MCP). It can quickly analyze code relationship graphs without the need to start an LSP, providing more contextual input for AI analysis without requiring any additional configuration.

```mermaid
graph TD
    A[main.py] --- S1[func_main] --- B[module_a.py]
    A --- S2[Handler] --- C[module_b.py]
    B --- S3[func_util] --- D[utils.py]
    C --- S3[func_util] --- D
    A --- S4[func_init] --- E[module_c.py]
    E --- S5[process] --- F[module_d.py]
    E --- S6[Processor] --- H[module_e.py]
    H --- S7[transform] --- I[module_f.py]
    I --- S3[func_util] --- D
```

## Installation

Install with pipx:

```shell
pipx install gossiphs-mcp
```

Start an MCP server:

```shell
# stdio by default
gossiphs-mcp server

# using SSE
gossiphs-mcp server --transport=sse
```

## Usage

> Using cursor as an example. Of course, other clients can also be used.

First, configure the server:

<img width="319" alt="Image" src="https://github.com/user-attachments/assets/65f855e1-a251-440b-9491-f7428ae52014" />

Then, when needed, cursor can automatically call gossiphs to analyze the impact scope of files and collaborate with other tools!

<img width="382" alt="Image" src="https://github.com/user-attachments/assets/7b86cedc-9651-42eb-9285-00a28b29ee7d" />
