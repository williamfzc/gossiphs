#!/bin/bash
set -e

# Configuration
GOSSIPHS_BIN="$(pwd)/target/debug/gossiphs"
ALIGNER_BIN="$(pwd)/target/debug/aligner"
BENCH_DIR="$(pwd)/eval/bench_repos"
mkdir -p "$BENCH_DIR"

echo ">>> Building tools..."
cargo build --bin gossiphs --bin aligner

# Function to run benchmark on a repo
# Usage: run_bench <repo_url> <repo_name>
run_bench() {
    local repo_url=$1
    local repo_name=$2
    local repo_path="$BENCH_DIR/$repo_name"

    echo "----------------------------------------"
    echo ">>> Processing $repo_name..."
    
    if [ ! -d "$repo_path" ]; then
        echo "Cloning $repo_name..."
        git clone "$repo_url" "$repo_path"
    else
        echo "$repo_name already exists."
    fi

    # Run comparison analysis using python script
    python3 eval/benchmark.py "$repo_path" "$repo_name"
}

# Run Benchmarks
# Rust
run_bench "https://github.com/tree-sitter/tree-sitter" "tree-sitter"
# Go
run_bench "https://github.com/gin-gonic/gin" "gin"
# TypeScript
run_bench "https://github.com/typescript-eslint/typescript-eslint" "typescript-eslint"
# Python
run_bench "https://github.com/tiangolo/fastapi" "fastapi"
# JavaScript
run_bench "https://github.com/lodash/lodash" "lodash"
# Java
run_bench "https://github.com/projectlombok/lombok" "lombok"
# Kotlin
run_bench "https://github.com/android/nowinandroid" "nowinandroid"
# Swift
run_bench "https://github.com/SwiftyJSON/SwiftyJSON" "SwiftyJSON"
# C#
run_bench "https://github.com/JamesNK/Newtonsoft.Json" "Newtonsoft.Json"
# C
run_bench "https://github.com/redis/redis" "redis"
# C++
run_bench "https://github.com/google/leveldb" "leveldb"

echo "========================================"
echo "All real world benchmarks finished."
echo "========================================"
