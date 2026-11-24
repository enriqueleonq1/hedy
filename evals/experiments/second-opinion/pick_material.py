#!/usr/bin/env python3
"""Test B's material (PLAN.md), chosen from the verdicts only.

Bad diffs fail the hidden check (not by an error or a timeout); good ones pass
it. Identical patches count once; at most 3 per case and class; cases take
turns so as many as possible are in. Planted errors (seeds.json) fill the bad
side up to the target.

    python3 pick_material.py <material.json> <diffs folder> [more folders...] [--per-class 20]
"""

import hashlib, json, random, sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
args = sys.argv[1:]
target = 20
if "--per-class" in args:
    i = args.index("--per-class"); target = int(args[i + 1]); del args[i:i + 2]
out, folders = Path(args[0]), [Path(a) for a in args[1:]]

seen, pools = set(), {True: defaultdict(list), False: defaultdict(list)}
for folder in folders:
    for verdict in sorted(folder.glob("*.json")):
        v = json.loads(verdict.read_text())
        patch = verdict.with_suffix(".patch")
        text = patch.read_text() if patch.exists() else ""
        if v.get("error") or not text.strip():
            continue
        digest = hashlib.sha256(text.encode()).hexdigest()
        if digest in seen:
            continue
        seen.add(digest)
        pools[bool(v["passed"])][v["case"]].append({
            "id": f"run:{patch.stem}", "case": v["case"], "patch": str(patch.resolve()),
            "bad": not v["passed"], "checkOutput": v.get("checkOutput", ""),
        })

rng = random.Random(7)


def take(pool, n):
    for items in pool.values():
        rng.shuffle(items)
    chosen, cases = [], sorted(pool)
    rng.shuffle(cases)
    for turn in range(3):
        for case in cases:
            if len(chosen) < n and len(pool[case]) > turn:
                chosen.append(pool[case][turn])
    return chosen


bad, good = take(pools[False], target), take(pools[True], target)
if len(bad) < target:
    seeds = json.loads((HERE / "seeds.json").read_text())
    rng.shuffle(seeds)
    for seed in seeds[: target - len(bad)]:
        bad.append({"id": seed["id"], "case": seed["case"], "patch": str((HERE / seed["patch"]).resolve()),
                    "bad": True, "cause": seed["cause"], "checkOutput": seed["checkOutput"]})
material = bad + good
out.write_text(json.dumps(material, ensure_ascii=False, indent=2) + "\n")
print(f"{sum(1 for m in material if m['bad'] and not m['id'].startswith('seed:'))} malos de corridas, "
      f"{sum(1 for m in material if m['id'].startswith('seed:'))} sembrados, {len(good)} buenos, "
      f"{len({m['case'] for m in material})} casos")
