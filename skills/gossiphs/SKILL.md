---
name: gossiphs
description: |
  Use when you need to find related files, understand code dependencies, or analyze impact scope.

  Triggers when the user asks about:
  - "Which files use this function/class?"
  - "What files will be affected if I change this?"
  - "Find code related to X"
  - "What does this file depend on?"
  - "Where is this symbol defined/referenced?"

  Uses gossiphs binary directly (no MCP required).
---

# Code Relations

Quickly discover file relationships in a codebase using semantic analysis.

## Prerequisites

Ensure `gossiphs` binary is available:
```bash
which gossiphs || echo "Install: cargo install gossiphs"
```

## Core Command

```bash
gossiphs relate --file <PATH> [--json]
```

Output is JSON array with:
- `name`: source file
- `related[]`: related files with `name`, `score`, `refs`, `defs`, `related_symbols`

## Usage Patterns

### Find related files

```bash
gossiphs relate --file src/auth/login.rs | jq '.[0].related[].name'
```

### Get top N related files by score

```bash
gossiphs relate --file src/lib.rs | jq '.[0].related | sort_by(-.score) | .[0:5][] | {name, score}'
```

### Find shared symbols between files

```bash
gossiphs relate --file src/lib.rs | jq '.[0].related[] | select(.name | contains("auth")) | .related_symbols[].symbol.name' | sort -u
```

### Impact analysis for a file change

```bash
gossiphs relate --file <FILE> | jq '.[0].related[] | "\(.name) (score: \(.score), refs: \(.refs))"'
```

## Parameters

| Flag | Description |
|------|-------------|
| `-p, --project-path` | Project root (default: `.`) |
| `--file` | Target file to analyze |
| `--depth` | Git history depth (default: 100) |
| `--strict` | Precise-first analysis |
| `--exclude-file-regex` | Exclude files matching pattern |

## Example Scenarios

### Scenario 1: Impact Analysis

User: "I want to refactor User class. What should I check?"

```
1. gossiphs relate --file src/models/user.py
2. Parse JSON output for related files
3. Report: "These files reference User symbols and may need updates..."
```

### Scenario 2: Find implementation

User: "Where is the authentication logic?"

```
1. Start from known entry: gossiphs relate --file src/routes/auth.rs
2. Follow related files to trace the flow
3. Report: "Auth flow: routes/auth.rs -> middleware/jwt.rs -> services/auth.rs"
```

### Scenario 3: Dependency discovery

User: "What does payment module depend on?"

```
1. gossiphs relate --file src/payment/
2. List incoming/outgoing connections
3. Summarize dependency graph
```

## Tips

- **First run may be slow** - builds index from git history
- **Subsequent runs are fast** - cached in `.gossiphs/`
- **Semantic over textual** - connections based on symbol semantics
- **Git history matters** - files changed together get stronger connections
