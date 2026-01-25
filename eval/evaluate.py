import json
import os
import subprocess

def get_commit_set(file_path, repo_path):
    try:
        cmd = f"git log --pretty=format:%H -- \"{file_path}\""
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True, cwd=repo_path)
        if result.returncode == 0:
            return set(result.stdout.splitlines())
    except Exception:
        pass
    return set()

def calculate_jaccard(set_a, set_b):
    if not set_a or not set_b:
        return 0.0
    intersection = len(set_a.intersection(set_b))
    union = len(set_a.union(set_b))
    return intersection / union if union > 0 else 0.0

def classify_link(link, scip_links, repo_path, file_commit_cache, logical_jaccard_threshold=0.2):
    """Return one of: confirmed / true_bonus / phantom."""
    if link in scip_links:
        return "confirmed", None
    src, dst = link
    if src not in file_commit_cache:
        file_commit_cache[src] = get_commit_set(src, repo_path)
    if dst not in file_commit_cache:
        file_commit_cache[dst] = get_commit_set(dst, repo_path)
    j = calculate_jaccard(file_commit_cache[src], file_commit_cache[dst])
    if j >= logical_jaccard_threshold:
        return "true_bonus", j
    return "phantom", j

def compute_coverage(links, all_src_files):
    """Coverage of source files that have at least one outgoing kept link."""
    if not all_src_files:
        return 0.0, 0, 0
    kept_src = {src for (src, _dst) in links}
    total = len(all_src_files)
    covered = len(kept_src.intersection(all_src_files))
    return covered / total, covered, total

def pick_score_cutoff(
    link_to_score,
    scip_links,
    repo_path,
    target_noise_ratio=0.3,
    logical_jaccard_threshold=0.2,
    max_candidates=2000,
    min_kept_links=0,
    min_src_coverage=0.0,
):
    """Pick a global score cutoff to control noise ratio (hallucination rate).

    - We scan candidate cutoffs from score quantiles (plus 0).
    - We classify links using SCIP hits and Git Jaccard.
    - We pick the lowest cutoff that achieves noise_ratio <= target_noise_ratio, else the one with best precision.
    """
    file_commit_cache = {}

    items = list(link_to_score.items())
    # Deterministic sampling to keep runtime bounded on huge repos.
    items.sort(key=lambda x: (-x[1], x[0][0], x[0][1]))
    if len(items) > max_candidates:
        items = items[:max_candidates]

    scores = sorted({s for _, s in items})
    if not scores:
        return 0

    # Candidate cutoffs: 0 and selected quantiles.
    quantiles = [0.0, 0.5, 0.7, 0.8, 0.9, 0.95]
    candidate_cutoffs = {0}
    for q in quantiles:
        idx = int((len(scores) - 1) * q)
        candidate_cutoffs.add(scores[idx])
    candidate_cutoffs = sorted(candidate_cutoffs)

    # Precompute labels for sampled links.
    link_label = {}
    for link, _ in items:
        label, _ = classify_link(link, scip_links, repo_path, file_commit_cache, logical_jaccard_threshold)
        link_label[link] = label

    all_src_files = {src for (src, _dst) in link_to_score.keys()}

    best = None
    for cutoff in candidate_cutoffs:
        kept = [(link, s) for link, s in items if s >= cutoff]
        if not kept:
            continue
        total = len(kept)
        phantom = sum(1 for link, _ in kept if link_label[link] == "phantom")
        confirmed = sum(1 for link, _ in kept if link_label[link] == "confirmed")
        bonus = sum(1 for link, _ in kept if link_label[link] == "true_bonus")
        hr = phantom / total
        precision = (confirmed + bonus) / total

        cov, cov_n, cov_total = compute_coverage([link for link, _ in kept], all_src_files)

        row = {
            "cutoff": cutoff,
            "total": total,
            "confirmed": confirmed,
            "true_bonus": bonus,
            "phantom": phantom,
            "hr": hr,
            "precision": precision,
            "src_coverage": cov,
            "src_covered": cov_n,
            "src_total": cov_total,
        }

        meets_noise = hr <= target_noise_ratio
        meets_kept = (min_kept_links <= 0) or (total >= min_kept_links)
        meets_cov = (min_src_coverage <= 0.0) or (cov >= min_src_coverage)

        if meets_noise and meets_kept and meets_cov:
            # pick the lowest cutoff that satisfies all constraints
            best = row
            break

        # fallback: maximize a utility that prefers higher precision and broader coverage
        # (still deterministic and monotonic in the common case)
        utility = row["precision"] - row["hr"] + 0.15 * row["src_coverage"]
        if best is None:
            best = row
            best["utility"] = utility
        else:
            best_utility = best.get("utility")
            if best_utility is None or utility > best_utility:
                best = row
                best["utility"] = utility

    return best

def report_after_cutoff(title, applied_cutoff, link_to_score, scip_links, repo_path, logical_jaccard_threshold=0.2, max_eval_links=5000):
    """Print quality stats after applying a score cutoff."""
    file_commit_cache = {}
    kept_links = [(link, s) for link, s in link_to_score.items() if s >= applied_cutoff]
    kept_links.sort(key=lambda x: (-x[1], x[0][0], x[0][1]))
    truncated = False
    if len(kept_links) > max_eval_links:
        kept_links = kept_links[:max_eval_links]
        truncated = True

    counts = {"confirmed": 0, "true_bonus": 0, "phantom": 0}
    for link, _ in kept_links:
        label, _ = classify_link(link, scip_links, repo_path, file_commit_cache, logical_jaccard_threshold=logical_jaccard_threshold)
        counts[label] += 1

    total_kept = sum(counts.values())
    noise_ratio = counts["phantom"] / total_kept if total_kept else 0.0
    effective_ratio = (counts["confirmed"] + counts["true_bonus"]) / total_kept if total_kept else 0.0

    all_src_files = {src for (src, _dst) in link_to_score.keys()}
    cov, cov_n, cov_total = compute_coverage([link for link, _ in kept_links], all_src_files)

    print(title)
    print(
        f"  Links after cutoff: {total_kept}"
        + (f" (truncated to top {max_eval_links} links for faster evaluation)" if truncated else "")
    )
    print(f"  - Confirmed by SCIP (strong evidence): {counts['confirmed']}")
    print(f"  - Supported by Git co-change (logical evidence): {counts['true_bonus']}")
    print(f"  - Likely noise (neither supported): {counts['phantom']}")
    print(f"  - Noise ratio (hallucination rate): {noise_ratio:.1%}")
    print(f"  - Effective ratio (non-noise): {effective_ratio:.1%}")
    print(f"  - Source coverage (src files with >=1 outgoing link): {cov:.1%} ({cov_n}/{cov_total})")

def evaluate(data_file, repo_path='.'):
    if not os.path.exists(data_file):
        print(f"Error: {data_file} not found.")
        return

    with open(data_file, 'r') as f:
        data = json.load(f)

    scip_rels = data['scip']['relations']
    gossiphs_rels = data['gossiphs']['relations']

    def get_file_link(r): return (r['src_file'], r['dst_file'])
    
    scip_file_links = set(get_file_link(r) for r in scip_rels)
    
    # 1. Group gossiphs relations by (src, dst) and take the max score
    gossiphs_link_to_score = {}
    for r in gossiphs_rels:
        link = get_file_link(r)
        score = r.get('score', 0)
        if link not in gossiphs_link_to_score or score > gossiphs_link_to_score[link]:
            gossiphs_link_to_score[link] = score

    # 2. Auto-pick score cutoffs under multiple target noise ratios.
    # Terms:
    # - score cutoff: keep links with score >= cutoff.
    # - noise ratio (hallucination rate): links that are neither confirmed by SCIP nor supported by Git Jaccard.
    scip_links = scip_file_links
    print(">>> Auto-picking score cutoffs for target noise ratios: 30% / 20% / 10% ...")
    total_links = len(gossiphs_link_to_score)
    # Prevent overly aggressive filtering from collapsing recall/coverage.
    min_kept_links = max(50, int(total_links * 0.01))
    min_src_coverage = 0.10
    for target in [0.30, 0.20, 0.10]:
        best = pick_score_cutoff(
            gossiphs_link_to_score,
            scip_links,
            repo_path,
            target_noise_ratio=target,
            logical_jaccard_threshold=0.2,
            max_candidates=2000,
            min_kept_links=min_kept_links,
            min_src_coverage=min_src_coverage,
        )
        if not isinstance(best, dict):
            print(f"  Target noise {target:.0%}: no suggested cutoff (insufficient data)")
            continue

        cutoff = int(best["cutoff"])
        print(
            f"  Target noise {target:.0%}: suggest score >= {cutoff} "
            f"(sample total={best['total']}, noise={best['hr']:.1%}, effective={best['precision']:.1%}, coverage={best['src_coverage']:.1%})"
        )
        report_after_cutoff(
            title=f"  >>> Quality report after applying score >= {cutoff}",
            applied_cutoff=cutoff,
            link_to_score=gossiphs_link_to_score,
            scip_links=scip_links,
            repo_path=repo_path,
            logical_jaccard_threshold=0.2,
            max_eval_links=5000,
        )

    # 3. Score Correlation Analysis
    print(">>> Analyzing Score vs. Accuracy Correlation...")
    file_commit_cache = {}
    
    # Buckets: 0-10, 10-50, 50-100, 100-200, 200+
    buckets = [
        (0, 10), (10, 50), (50, 100), (100, 500), (500, float('inf'))
    ]
    bucket_stats = {b: {'total': 0, 'confirmed': 0, 'true_bonus': 0, 'phantom': 0} for b in buckets}

    for link, score in gossiphs_link_to_score.items():
        # Find bucket
        bucket = None
        for b in buckets:
            if b[0] <= score < b[1]:
                bucket = b
                break
        
        if not bucket: continue
        
        bucket_stats[bucket]['total'] += 1
        
        # 1. Is it a physical link (Confirmed)?
        if link in scip_file_links:
            bucket_stats[bucket]['confirmed'] += 1
        else:
            # 2. Check for Logical Evidence
            src, dst = link
            if src not in file_commit_cache: file_commit_cache[src] = get_commit_set(src, repo_path)
            if dst not in file_commit_cache: file_commit_cache[dst] = get_commit_set(dst, repo_path)
            
            jaccard = calculate_jaccard(file_commit_cache[src], file_commit_cache[dst])
            if jaccard >= 0.2:
                bucket_stats[bucket]['true_bonus'] += 1
            else:
                bucket_stats[bucket]['phantom'] += 1

    print("\n" + "="*70)
    print("       SCORE CORRELATION ANALYSIS (Score vs. Quality)")
    print("="*70)
    print(f"{'Score Range':<15} | {'Total':<8} | {'Confirmed':<10} | {'True Bonus':<10} | {'Phantom':<8} | {'HR':<8}")
    print("-" * 70)
    
    total_confirmed = 0
    total_phantoms = 0
    total_links = 0

    for b in buckets:
        stats = bucket_stats[b]
        range_str = f"{b[0]}-{b[1]}" if b[1] != float('inf') else f"{b[0]}+"
        hr = (stats['phantom'] / stats['total'] * 100) if stats['total'] > 0 else 0
        print(f"{range_str:<15} | {stats['total']:<8} | {stats['confirmed']:<10} | {stats['true_bonus']:<10} | {stats['phantom']:<8} | {hr:>6.1f}%")
        
        total_confirmed += stats['confirmed']
        total_phantoms += stats['phantom']
        total_links += stats['total']

    print("-" * 70)
    overall_hr = (total_phantoms / total_links * 100) if total_links > 0 else 0
    print(f"{'OVERALL':<15} | {total_links:<8} | {total_confirmed:<10} | {'-':<10} | {total_phantoms:<8} | {overall_hr:>6.1f}%")
    print("="*70 + "\n")

if __name__ == "__main__":
    import sys
    data_path = sys.argv[1] if len(sys.argv) > 1 else 'eval/aligned_data.json'
    repo_path = sys.argv[2] if len(sys.argv) > 2 else '.'
    evaluate(data_path, repo_path)
