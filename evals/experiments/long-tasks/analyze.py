#!/usr/bin/env python3
"""Test C's analysis (PLAN.md): each arm, session by session, against solo.

Reads `hedy-evals --long` results (JSON lines, several files). A run is one
case, arm, repeat and start; its sessions are its lines. Runs under the
third-party layer's account are the `layer` arm.

    python3 analyze.py <evals/long> results.jsonl [more.jsonl ...] [--layer-account gentle]
"""

import json, random, sys
from collections import defaultdict
from pathlib import Path

args = sys.argv[1:]
layer_account = "gentle"
if "--layer-account" in args:
    i = args.index("--layer-account"); layer_account = args[i + 1]; del args[i:i + 2]
suite, paths = Path(args[0]), args[1:]
specs = {p.parent.name: json.loads(p.read_text()) for p in suite.glob("*/case.json")}

runs = defaultdict(dict)
for path in paths:
    for line in open(path):
        if not line.strip():
            continue
        r = json.loads(line)
        arm = r["variant"].split("+long:")[1]
        if r["account"] == layer_account:
            arm = "layer"
        runs[(r["case"], arm, r.get("model"), r["attempt"], r["startedAt"])][r["long"]["session"]] = r

def weighted(r):
    """Tokens at input price: Claude's output costs 5×, a cache read 0.1×, a cache write 1.25×."""
    return r.get("inputTokens", 0) + 5 * r.get("outputTokens", 0) + 0.1 * r.get("cacheReadTokens", 0) + 1.25 * r.get("cacheWriteTokens", 0)


# A session cut at its cap has no price from the CLI: price it at the model's rate, the median
# over its sessions that have one (a few long sessions are billed above the token rate).
rates = defaultdict(list)
for sessions in runs.values():
    for r in sessions.values():
        if r.get("costUsd") and weighted(r):
            rates[r.get("model")].append(r["costUsd"] / weighted(r))
rate = {m: sorted(ks)[len(ks) // 2] for m, ks in rates.items()}


def cost(r):
    if r.get("costUsd") is not None:
        return r["costUsd"]
    return weighted(r) * rate.get(r.get("model"), 0.0)


complete, broken = {}, 0
for key, sessions in runs.items():
    if len(sessions) == 3 and not any(s.get("error") for s in sessions.values()) and all(s["tokens"] for s in sessions.values()):
        complete[key] = sessions
    else:
        broken += 1


def met(session, ids=None):
    reqs = session["long"]["reqs"]
    keys = [k for k in reqs if k != "REG" and (ids is None or k in ids)]
    return sum(reqs[k] for k in keys) / max(len(keys), 1)


def regressions(sessions, case):
    changed = set(specs[case].get("changed", []))
    count = 0
    for a, b in ((1, 2), (2, 3)):
        before, after = sessions[a]["long"]["reqs"], sessions[b]["long"]["reqs"]
        for k, ok in before.items():
            if ok and not after.get(k, False) and not (b == 3 and k in changed):
                count += 1
    return count


def row(case, sessions):
    spec = specs[case]
    s3 = sessions[3]
    reqs3 = s3["long"]["reqs"]
    linked = spec.get("linked", [])
    return {
        "met": [met(sessions[n]) for n in (1, 2, 3)],
        "regressions": regressions(sessions, case),
        "exact": met(s3, set(spec.get("exact", []))),
        "change": met(s3, set(spec.get("changed", []))),
        "linked": float(all(reqs3.get(k, False) for k in linked)) if linked else None,
        "reg": float(reqs3.get("REG", False)),
        "reads": [sessions[n]["long"].get("readsBeforeEdit") for n in (2, 3)],
        "stopped": sum(sessions[n]["long"]["stopped"] for n in (1, 2, 3)),
        "cost": sum(cost(sessions[n]) for n in (1, 2, 3)),
        "tokens": sum(sessions[n]["tokens"] for n in (1, 2, 3)),
    }


table = defaultdict(lambda: defaultdict(list))  # (model, arm) -> case -> rows
for (case, arm, model, _, _), sessions in complete.items():
    table[(model, arm)][case].append(row(case, sessions))


def mean(xs):
    xs = [x for x in xs if x is not None]
    return sum(xs) / len(xs) if xs else float("nan")


def boot_diff(arm_rows, solo_rows, n=10000, seed=7):
    """95 % interval of (arm − solo) in requirements met after session 3, resampling cases."""
    cases = [c for c in arm_rows if c in solo_rows]
    if not cases:
        return float("nan"), float("nan")
    rng, out = random.Random(seed), []
    for _ in range(n):
        pick = [rng.choice(cases) for _ in cases]
        out.append(mean([mean([r["met"][2] for r in arm_rows[c]]) - mean([r["met"][2] for r in solo_rows[c]]) for c in pick]))
    out.sort()
    return out[int(0.025 * n)], out[int(0.975 * n) - 1]


print(f"{len(complete)} corridas completas, {broken} incompletas o con error (fuera)")
for model, k in sorted(rate.items(), key=str):
    print(f"  {model}: $ {k * 1e6:.3f} por millón de tokens de entrada (mediana de sus sesiones con precio)")
print()
for model in sorted({m for m, _ in table}, key=str):
    print(f"== {model}")
    print(f"{'brazo':<8} {'n':>3} {'S1':>5} {'S2':>5} {'S3':>5} {'Δ S3 vs solo':>13} {'IC 95 %':>17} {'regres.':>8} {'exactos':>8} {'cambio':>7} {'enlaz.':>7} {'REG':>5} {'lect. S2/S3':>12} {'cortes':>7} {'$':>8} {'$/req':>8}")
    solo = table.get((model, "solo"), {})
    for arm in ("solo", "repeat", "doc", "layer", "flow"):
        cases = table.get((model, arm))
        if not cases:
            continue
        rows = [r for rs in cases.values() for r in rs]
        s = [mean([r["met"][i] for r in rows]) for i in range(3)]
        solo_s3 = mean([r["met"][2] for rs in solo.values() for r in rs]) if solo else float("nan")
        lo, hi = boot_diff(cases, solo) if arm != "solo" else (float("nan"), float("nan"))
        cost = mean([r["cost"] for r in rows])
        per_req = cost / max(s[2] * 24, 1e-9)
        reads = f"{mean([r['reads'][0] for r in rows]):.0f}/{mean([r['reads'][1] for r in rows]):.0f}"
        delta = "" if arm == "solo" else f"{s[2] - solo_s3:+.0%}"
        ci = "" if arm == "solo" else f"[{lo:+.0%}, {hi:+.0%}]"
        print(f"{arm:<8} {len(rows):>3} {s[0]:>5.0%} {s[1]:>5.0%} {s[2]:>5.0%} {delta:>13} {ci:>17} {mean([r['regressions'] for r in rows]):>8.1f} "
              f"{mean([r['exact'] for r in rows]):>8.0%} {mean([r['change'] for r in rows]):>7.0%} {mean([r['linked'] for r in rows]):>7.0%} "
              f"{mean([r['reg'] for r in rows]):>5.0%} {reads:>12} {mean([r['stopped'] for r in rows]):>7.1f} {cost:>8.4f} {per_req:>8.4f}")
    # Descriptive, not in the plan: of the runs whose first session left work (under 20 of 24),
    # how many got to 20 by the third.
    print("\n  primera sesión a medias (< 20/24) que llegan a 20/24 tras S3:")
    for arm in ("solo", "repeat", "doc", "layer", "flow"):
        rows = [r for rs in table.get((model, arm), {}).values() for r in rs if r["met"][0] < 20 / 24]
        if rows:
            print(f"    {arm:<8} {sum(r['met'][2] >= 20 / 24 for r in rows)}/{len(rows)}")
    print("\n  por caso (requisitos tras S3, media):")
    for case in sorted(specs):
        cells = []
        for arm in ("solo", "repeat", "doc", "layer", "flow"):
            rs = table.get((model, arm), {}).get(case, [])
            cells.append(f"{mean([r['met'][2] for r in rs]):>6.0%}" if rs else f"{'—':>6}")
        print(f"    {case:<24} " + " ".join(cells))
    print()
