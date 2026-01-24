import json
import os

def evaluate(data_file):
    if not os.path.exists(data_file):
        print(f"Error: {data_file} not found.")
        return

    with open(data_file, 'r') as f:
        data = json.load(f)

    scip_syms = data['scip']['symbols']
    gossiphs_syms = data['gossiphs']['symbols']
    scip_rels = data['scip']['relations']
    gossiphs_rels = data['gossiphs']['relations']

    def normalize_scip_name(scip_symbol_str):
        if '/' in scip_symbol_str:
            parts = scip_symbol_str.split('/')
            name_part = parts[-1]
            return name_part.replace('().', '').replace('()', '').replace('#', '').strip('.')
        return scip_symbol_str

    # 1. Symbol Analysis
    scip_normalized = set(f"{s['file']}:{normalize_scip_name(s['name'])}" for s in scip_syms)
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
    
    # 2. Relation Analysis (Symbol Level)
    def normalize_rel(r):
        sym = normalize_scip_name(r['symbol_name'])
        return f"{r['src_file']} -> {r['dst_file']} ({sym})"

    scip_rel_norm = set(normalize_rel(r) for r in scip_rels)
    gossiphs_rel_norm = set(normalize_rel(r) for r in gossiphs_rels)
    rel_hits = scip_rel_norm.intersection(gossiphs_rel_norm)

    # 3. File-to-File Analysis (The "Main Roads")
    def get_file_link(r): return f"{r['src_file']} -> {r['dst_file']}"
    
    scip_file_links = set(get_file_link(r) for r in scip_rels)
    gossiphs_file_links = set(get_file_link(r) for r in gossiphs_rels)
    
    file_link_hits = scip_file_links.intersection(gossiphs_file_links)
    file_link_extra = gossiphs_file_links - scip_file_links
    file_link_missed = scip_file_links - gossiphs_file_links
    
    print("========================================")
    print("       SYMBOLS EVALUATION (Static)")
    print("========================================")
    print(f"SCIP Unique Symbols:     {len(scip_normalized)}")
    print(f"gossiphs Unique Symbols: {len(gossiphs_normalized)}")
    print(f"Symbol Recall:           {len(sym_hits)/len(scip_normalized):.2%}" if scip_normalized else "0%")

    print("\n========================================")
    print("    FILE-LEVEL LINKS (Architecture)")
    print("========================================")
    print(f"SCIP File-to-File Links: {len(scip_file_links)}")
    print(f"gossiphs File-to-File:   {len(gossiphs_file_links)}")
    print(f"File-Link Recall:        {len(file_link_hits)/len(scip_file_links):.2%}" if scip_file_links else "0%")
    print(f"Extra File Links:        {len(file_link_extra)} (Found by gossiphs only)")
    if file_link_extra:
        print("  (Sample extra links - Logical coupling/Cross-lang):")
        for r in list(file_link_extra)[:5]: print(f"    + {r}")

    print("\n========================================")
    print("    DETAILED RELATION EVALUATION")
    print("========================================")
    print(f"Relation Recall (Symbol-level): {len(rel_hits)/len(scip_rel_norm):.2%}" if scip_rel_norm else "0%")
    
    print("\n========================================")
    print("             ANALYSIS")
    print("========================================")
    if file_link_missed:
        print(f"SCIP reported {len(file_link_missed)} file links that gossiphs missed.")
        print("Sample missed links:")
        for r in list(file_link_missed)[:5]: print(f"    - {r}")
    print("========================================")

if __name__ == "__main__":
    evaluate('eval/aligned_data.json')
