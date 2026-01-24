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
        return f"{src} -> {dst}"

    scip_links_set = set(get_link_str(l, True) for l in scip_file_links_raw)
    gossiphs_links_set = set(get_link_str(l, False) for l in gossiphs_file_links_raw)
    
    link_hits = scip_links_set.intersection(gossiphs_links_set)
    link_precision = len(link_hits) / len(gossiphs_links_set) if gossiphs_links_set else 0
    link_recall = len(link_hits) / len(scip_links_set) if scip_links_set else 0
    
    print("\n" + "="*60)
    print(f"COMPARISON REPORT (FILE DIMENSION): {output_tag}")
    print(f"Path: {repo_path}")
    print("="*60)
    print(f"[1. Symbol Precision]")
    print(f"  - gossiphs symbols found: {len(gossiphs_normalized)}")
    print(f"  - Precision:   {sym_precision:.2%}")
    
    print(f"\n[2. File-to-File Level]")
    print(f"  - Baseline links (SCIP):   {len(scip_links_set)}")
    print(f"  - gossiphs links:     {len(gossiphs_links_set)}")
    print(f"  - Precision:   {link_precision:.2%}")
    print(f"  - Recall:      {link_recall:.2%}")
    print(f"  - Bonus logical links: {len(gossiphs_links_set - scip_links_set)}")
    print("\n" + "="*60)

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: python3 benchmark.py <repo_path> <tag>")
        sys.exit(1)
    evaluate(sys.argv[1], sys.argv[2])
