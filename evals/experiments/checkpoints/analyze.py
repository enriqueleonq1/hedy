"""Summarises the checkpoints test (PLAN.md): each arm's pass rate and cost,
and the paired differences between arms, by case.

    python3 analyze.py <results.jsonl> [more.jsonl ...]
"""

import json
import random
import statistics
import sys
from collections import defaultdict

ARMS = ["solo", "informed", "plan", "approval"]
PAIRS = [("plan", "approval"), ("plan", "informed"), ("plan", "solo"), ("approval", "solo")]
SEED = 7
RESAMPLES = 10_000


def load(paths):
    runs = []
    for path in paths:
        with open(path) as f:
            runs.extend(json.loads(line) for line in f if line.strip())
    return [r for r in runs if r.get("checkpoint")]


def by_case(runs, arm, key):
    """Mean of `key` per case for `arm`."""
    groups = defaultdict(list)
    for r in runs:
        if r["checkpoint"]["arm"] == arm:
            groups[r["case"]].append(key(r))
    return {case: statistics.mean(values) for case, values in groups.items()}


def bootstrap(diffs):
    """95 % interval of the mean of per-case differences."""
    rng = random.Random(SEED)
    means = sorted(
        statistics.mean(rng.choice(diffs) for _ in diffs) for _ in range(RESAMPLES)
    )
    return means[int(0.025 * RESAMPLES)], means[int(0.975 * RESAMPLES) - 1]


def passed(r):
    return 100.0 if r["passed"] else 0.0


def cost(r):
    return r.get("costUsd") or 0.0


def table(runs, cases, title):
    print(f"\n## {title} ({len(cases)} cases)\n")
    print("| arm | runs | passed | $ / run | × plan | seconds | corrected | errors |")
    print("|---|---|---|---|---|---|---|---|")
    plan_cost = statistics.mean(by_case(runs, "plan", cost)[c] for c in cases)
    for arm in ARMS:
        mine = [r for r in runs if r["checkpoint"]["arm"] == arm and r["case"] in cases]
        if not mine:
            continue
        rate = statistics.mean(by_case(mine, arm, passed)[c] for c in cases)
        dollars = statistics.mean(by_case(mine, arm, cost)[c] for c in cases)
        secs = statistics.mean(r["durationMs"] / 1000 for r in mine)
        corrected = sum(r["checkpoint"]["corrected"] for r in mine)
        errors = sum(1 for r in mine if r.get("error"))
        fixed = f"{corrected}/{len(mine)}" if arm in ("plan", "approval") else ""
        print(
            f"| {arm} | {len(mine)} | {rate:.0f} % | {dollars:.3f} | "
            f"{dollars / plan_cost:.2f} | {secs:.0f} | {fixed} | {errors} |"
        )
    print("\n| difference | passed (points) | 95 % | cost ratio |")
    print("|---|---|---|---|")
    for a, b in PAIRS:
        pa, pb = by_case(runs, a, passed), by_case(runs, b, passed)
        ca, cb = by_case(runs, a, cost), by_case(runs, b, cost)
        both = [c for c in cases if c in pa and c in pb]
        if not both:
            continue
        diffs = [pa[c] - pb[c] for c in both]
        low, high = bootstrap(diffs)
        ratio = statistics.mean(ca[c] for c in both) / statistics.mean(cb[c] for c in both)
        print(
            f"| {a} − {b} | {statistics.mean(diffs):+.0f} | [{low:+.0f}, {high:+.0f}] | {ratio:.2f}× |"
        )


def main():
    runs = load(sys.argv[1:])
    cases = sorted({r["case"] for r in runs})
    print("By case (passed / runs, mean $):\n")
    print("| case | " + " | ".join(ARMS) + " |")
    print("|---" * (len(ARMS) + 1) + "|")
    for case in cases:
        cells = []
        for arm in ARMS:
            mine = [r for r in runs if r["case"] == case and r["checkpoint"]["arm"] == arm]
            if mine:
                ok = sum(r["passed"] for r in mine)
                cells.append(f"{ok}/{len(mine)} · {statistics.mean(map(cost, mine)):.3f}")
            else:
                cells.append("")
        print(f"| {case} | " + " | ".join(cells) + " |")
    table(runs, cases, "All cases")
    solo = by_case(runs, "solo", passed)
    ambiguous = [c for c in cases if solo.get(c, 100) < 50]
    if ambiguous and ambiguous != cases:
        table(runs, ambiguous, "Cases where solo passes under half")


if __name__ == "__main__":
    main()
