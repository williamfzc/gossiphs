import json
import subprocess
import sys
import os

def run_cmd(cmd, cwd=None):
    print(f"Executing: {cmd}")
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True, cwd=cwd)
    if result.returncode != 0:
        print(f"Error executing {cmd}: {result.stderr}")
        return False
    return True

def normalize_scip_name(scip_symbol_str):
    if not scip_symbol_str: return ""
    import re
    matches = re.findall(r'[#\.]([a-zA-Z0-9_]+)', scip_symbol_str)
    if matches:
        return matches[-1]
    
    if '/' in scip_symbol_str:
        parts = scip_symbol_str.split('/')
        name_part = parts[-1]
        return name_part.replace('().', '').replace('()', '').replace('#', '').replace('.', '').replace('`', '').strip()
    return scip_symbol_str

def evaluate(repo_path, output_tag):
    repo_path = os.path.abspath(repo_path)
    eval_root = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.dirname(eval_root)
    output_dir = os.path.join(eval_root, "output", output_tag)
    os.makedirs(output_dir, exist_ok=True)
    
    scip_file = os.path.join(output_dir, "index.scip")
    gossiphs_index = os.path.join(output_dir, "gossiphs.index")
    aligned_file = os.path.join(eval_root, f"aligned_{output_tag}.json")

    # Step 1: Run Indexers
    print(f">>> [{output_tag}] Running Indexers...")
    has_scip = False
    
    # 1.1 Rust
    if os.path.exists(os.path.join(repo_path, "Cargo.toml")):
        if run_cmd(f"rust-analyzer scip {repo_path} --output {scip_file}"):
            has_scip = True
    
    # 1.2 TypeScript / JavaScript
    elif os.path.exists(os.path.join(repo_path, "package.json")):
        if run_cmd(f"scip-typescript index {repo_path} --output {scip_file}"):
            has_scip = True

    # 1.3 Java / Kotlin
    elif os.path.exists(os.path.join(repo_path, "build.gradle")) or \
         os.path.exists(os.path.join(repo_path, "build.gradle.kts")) or \
         os.path.exists(os.path.join(repo_path, "pom.xml")):
        if run_cmd(f"scip-java index --output {scip_file}", cwd=repo_path):
            has_scip = True
            
    # 1.4 Go
    elif os.path.exists(os.path.join(repo_path, "go.mod")):
        scip_go_bin = "scip-go"
        gopath_res = subprocess.run("go env GOPATH", shell=True, capture_output=True, text=True)
        gopath = gopath_res.stdout.strip() if gopath_res.returncode == 0 else ""
        paths_to_check = [
            os.path.join(gopath, "bin", "scip-go"),
            "/Users/bytedance/go/1.22.12/bin/scip-go",
            os.path.expanduser("~/go/bin/scip-go")
        ]
        for p in paths_to_check:
            if os.path.exists(p):
                scip_go_bin = p
                break
            
        if run_cmd(f"{scip_go_bin} -o {scip_file}", cwd=repo_path):
            has_scip = True
    
    # Run gossiphs
    gossiphs_bin = os.path.join(project_root, "target", "debug", "gossiphs")
    if not run_cmd(f"{gossiphs_bin} relation2 -p {repo_path} --index-file {gossiphs_index}"):
        return

    if not has_scip:
        print(f"!!! SCIP indexer not available or failed for this project type. Skipping comparison.")
        return

    # Step 2: Align
    print(f">>> [{output_tag}] Aligning Data...")
    aligner_bin = os.path.join(project_root, "target", "debug", "aligner")
    if not run_cmd(f"{aligner_bin} {scip_file} {gossiphs_index}"):
        return
    
    default_aligned = os.path.join(eval_root, "aligned_data.json")
    if os.path.exists(default_aligned):
        if os.path.exists(aligned_file):
            os.remove(aligned_file)
        os.rename(default_aligned, aligned_file)

    # Step 3: Parse and Evaluate
    if not os.path.exists(aligned_file):
        print(f"Error: {aligned_file} not found.")
        return
        
    with open(aligned_file, 'r') as f:
        data = json.load(f)

    scip_syms = data['scip']['symbols']
    gossiphs_syms = data['gossiphs']['symbols']
    
    # New: Use file_links from aligned data
    scip_file_links_raw = data['scip'].get('file_links', [])
    gossiphs_file_links_raw = data['gossiphs'].get('file_links', [])

    def norm_path(p):
        p_abs = os.path.abspath(os.path.join(project_root, p))
        if p_abs.startswith(repo_path):
            return os.path.relpath(p_abs, repo_path)
        return p

    # Symbol Analysis (Still useful for context)
    scip_normalized = set(f"{norm_path(s['file'])}:{normalize_scip_name(s['name'])}" for s in scip_syms)
    gossiphs_normalized = set()
    for s in gossiphs_syms:
        if s['file'] != "unknown":
            gossiphs_normalized.add(f"{s['file']}:{s['name']}")
        else:
            gossiphs_normalized.add(f"ANY:{s['name']}")

    # Grep Baseline calculation (Naive symbol matching)
    print(f">>> [{output_tag}] Calculating Grep Baseline...")
    def get_base_name(scip_name):
        return normalize_scip_name(scip_name)

    # 1. Collect all defined symbols and their files
    name_to_def_files = {}
    for s in scip_syms:
        # In SCIP, we can infer DEF if it has a specific format or if we use the scip_file_links
        # But a simpler way: if it's in scip_symbols, it's a known symbol.
        # Let's use the ones that are actually used in scip_file_links as the "truth"
        pass
    
    # Actually, let's use the symbols that are DSTs in scip_file_links
    truth_defs = {} # name -> set of files
    for l in scip_file_links_raw:
        dst = norm_path(l['dst_file'])
        # We don't have the symbol name in file_links, but we have it in 'relations'
        pass
    
    # Let's use 'relations' from aligned data if available
    scip_relations = data['scip'].get('relations', [])
    for r in scip_relations:
        name = get_base_name(r['symbol_name'])
        dst = norm_path(r['dst_file'])
        if name not in truth_defs:
            truth_defs[name] = set()
        truth_defs[name].add(dst)

    # 2. For each file in the repo, check which symbols it "mentions"
    grep_links_set = set()
    repo_files = []
    for root, _, files in os.walk(repo_path):
        for f in files:
            if f.endswith(('.go', '.rs', '.ts', '.js', '.py', '.cpp', '.c', '.h', '.hpp', '.cc')):
                repo_files.append(os.path.relpath(os.path.join(root, f), repo_path))

    # Pre-compile regex for all base names to speed up
    if truth_defs:
        import re
        # Filter out very short or too common names to avoid regex explosion
        valid_names = [n for n in truth_defs.keys() if len(n) > 3]
        if valid_names:
            # Match whole words only
            # Using a single regex for all names might be too big, let's chunk it
            chunk_size = 100
            name_chunks = [valid_names[i:i + chunk_size] for i in range(0, len(valid_names), chunk_size)]
            regex_chunks = [re.compile(r'\b(' + '|'.join(map(re.escape, chunk)) + r')\b') for chunk in name_chunks]

            for rel_path in repo_files:
                abs_path = os.path.join(repo_path, rel_path)
                try:
                    with open(abs_path, 'r', errors='ignore') as f:
                        content = f.read()
                        for rgx in regex_chunks:
                            for match in rgx.finditer(content):
                                name = match.group(1)
                                for dst_file in truth_defs[name]:
                                    if rel_path != dst_file:
                                        grep_links_set.add(f"{rel_path} -> {dst_file}")
                except Exception as e:
                    print(f"Warning: could not read {abs_path}: {e}")

    sym_hits = []
    for g_key in gossiphs_normalized:
        if g_key.startswith("ANY:"):
            name = g_key.split(":")[1]
            if any(s_key.endswith(f":{name}") for s_key in scip_normalized):
                sym_hits.append(g_key)
        elif g_key in scip_normalized:
            sym_hits.append(g_key)
    
    sym_precision = len(sym_hits) / len(gossiphs_normalized) if gossiphs_normalized else 0

    # File Dimension Comparison (Primary focus)
    def get_link_str(l, is_scip=False):
        src = norm_path(l['src_file']) if is_scip else l['src_file']
        dst = norm_path(l['dst_file']) if is_scip else l['dst_file']
        
        # Filter: only keep links within the repository scope
        # (Exclude external libraries, caches, or system files)
        if src.startswith('/') or src.startswith('..') or \
           dst.startswith('/') or dst.startswith('..'):
            return None
            
        return f"{src} -> {dst}"

    scip_links_raw = [get_link_str(l, True) for l in scip_file_links_raw]
    scip_links_set = set(l for l in scip_links_raw if l is not None)
    
    gossiphs_links_raw = [get_link_str(l, False) for l in gossiphs_file_links_raw]
    gossiphs_links_set = set(l for l in gossiphs_links_raw if l is not None)
    
    link_hits = scip_links_set.intersection(gossiphs_links_set)
    link_precision = len(link_hits) / len(gossiphs_links_set) if gossiphs_links_set else 0
    link_recall = len(link_hits) / len(scip_links_set) if scip_links_set else 0
    
    # Grep Precision/Recall
    grep_hits = scip_links_set.intersection(grep_links_set)
    grep_precision = len(grep_hits) / len(grep_links_set) if grep_links_set else 0
    grep_recall = len(grep_hits) / len(scip_links_set) if scip_links_set else 0

    print("\n" + "="*60)
    print(f"COMPARISON REPORT: {output_tag}")
    print(f"Path: {repo_path}")
    print("="*60)
    print(f"[1. Overall Metrics]")
    print(f"  - Baseline (SCIP) Real Links:  {len(scip_links_set)}")
    print(f"  - Grep (Naive) Links:          {len(grep_links_set)}")
    print(f"  - gossiphs (Heuristic) Links:  {len(gossiphs_links_set)}")
    
    print(f"\n[2. Precision (Against SCIP)]")
    print(f"  - Grep Precision:     {grep_precision:.2%}")
    print(f"  - gossiphs Precision: {link_precision:.2%}")
    
    print(f"\n[3. Recall (Against SCIP)]")
    print(f"  - Grep Recall:        {grep_recall:.2%}")
    print(f"  - gossiphs Recall:    {link_recall:.2%}")
    
    print(f"\n[4. Architectural Context]")
    print(f"  - Bonus Logical Links (gossiphs): {len(gossiphs_links_set - scip_links_set)}")
    print(f"  - Avoided Noise Links (vs Grep):  {len(grep_links_set - gossiphs_links_set)}")
    print("="*60 + "\n")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: python3 benchmark.py <repo_path> <tag>")
        sys.exit(1)
    evaluate(sys.argv[1], sys.argv[2])
