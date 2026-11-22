#!/usr/bin/env python3
"""Flows against one agent, by cost per solved task (docs/decisions.md §50).

Reads hedy-evals results (JSON lines, several files) and prints, per flow,
the pass rate, tokens and cost per run, the cost per solved task and the AI
steps per run, then the passes per case.

    python3 compare.py results.jsonl [more.jsonl ...]
"""

import json, sys
from collections import defaultdict
from statistics import mean

runs = []
for path in sys.argv[1:]:
    for line in open(path):
        r = json.loads(line)
        if r.get("error"):
            continue
        r["flow"] = r["variant"].split("+flow:")[1] if "+flow:" in r["variant"] else "none"
        runs.append(r)
keep = ["none", "full", "ligero", "acotado", "tdd", "tdd-acotado"]
by = defaultdict(list)
for r in runs:
    if r["flow"] in keep:
        by[r["flow"]].append(r)
cases = sorted({r["case"] for r in runs if r["flow"] in keep})
print("  variante       éxito   tokens/corrida  $/corrida  $/resuelta  pasos IA/corrida")
for f in keep:
    rs = by[f]
    if not rs: continue
    cost = sum(r.get("costUsd") or 0 for r in rs); ok = sum(r["passed"] for r in rs)
    steps = mean(sum(1 for s in r.get("steps", []) if s["kind"] == "role") for r in rs)
    print(f"  {f:<13} {ok/len(rs):>5.0%} {mean(r['tokens'] for r in rs):>14,.0f} {cost/len(rs):>10.4f} {cost/ok if ok else 0:>10.4f} {steps:>10.1f}   ({len(rs)})")
print()
print("  " + "caso".ljust(22) + "".join(f[:11].rjust(12) for f in keep))
for c in cases:
    print("  " + c.ljust(22) + "".join((lambda rs: f"{sum(r['passed'] for r in rs)}/{len(rs)}" if rs else "-")([r for r in by[f] if r["case"] == c]).rjust(12) for f in keep))
