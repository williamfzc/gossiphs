import json
import os

def analyze_fn(file_path):
    with open(file_path, 'r') as f:
        data = json.load(f)
    
    g_links = set((l['src_file'], l['dst_file']) for l in data['gossiphs']['file_links'])
    s_links = set((l['src_file'], l['dst_file']) for l in data['scip']['file_links'])
    
    # In scip but not in gossiphs (False Negatives)
    fn_links = s_links - g_links
    
    print(f"File: {file_path}")
    print(f"Total SCIP links: {len(s_links)}")
    print(f"Total gossiphs links: {len(g_links)}")
    print(f"Missing links (FN): {len(fn_links)}")
    print("\nSample missing links (FN):")
    for src, dst in list(fn_links)[:30]:
        print(f"  {src} -> {dst}")

if __name__ == "__main__":
    analyze_fn("eval/aligned_gin.json")
