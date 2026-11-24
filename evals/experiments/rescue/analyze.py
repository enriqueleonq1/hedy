#!/usr/bin/env python3
"""Test A's analysis (PLAN.md): each arm against base, paired by run.

Reads `hedy-evals --rescue` results (JSON lines, several files). A run is the
four arms sharing a case, attempt and start. Runs whose first attempt ended in
an error are left out; an arm that changed the project's tests is left out of
its own comparison.

    python3 analyze.py results.jsonl [more.jsonl ...]
"""

import json, random, sys
from collections import defaultdict

ARMS = ["base", "base+rescue:same", "base+rescue:fresh", "base+rescue:retry"]
SHORT = {a: a.split(":")[-1] for a in ARMS}

runs = defaultdict(dict)
for path in sys.argv[1:]:
    for line in open(path):
        if line.strip():
            r = json.loads(line)
            runs[(r["case"], r["attempt"], r["startedAt"])][r["variant"]] = r

valid, errors = [], 0
for key, arms in runs.items():
    base = arms.get("base")
    if not base or len(arms) < 4:
        continue
    if base.get("error") or base["tokens"] == 0:
        errors += 1
        continue
    valid.append(arms)

cases = sorted({a["base"]["case"] for a in valid})
fired = [a for a in valid if a["base"]["rescue"]["testsFailed"]]
print(f"{len(valid)} corridas válidas ({errors} con error en el primer intento, fuera), {len(cases)} casos")
print(f"disparos (tests del proyecto fallan tras el 1er intento): {len(fired)}/{len(valid)} = {len(fired)/max(len(valid),1):.0%}\n")


def usable(arm):
    return not arm["rescue"]["testsEdited"] and not (arm.get("error") and arm["rescue"]["second"])


def compare(name):
    rescues = breaks = invalid = errors = 0
    extra_tokens = extra_cost = 0
    extra_untriggered = 0
    rescued_cases = set()
    per_case = defaultdict(lambda: [0, 0, 0])  # arm passes, base passes, runs
    for arms in valid:
        base, arm = arms["base"], arms[name]
        if not usable(arm):
            invalid += 1
            continue
        if arm.get("error"):
            errors += 1
        c = per_case[base["case"]]
        c[0] += arm["passed"]; c[1] += base["passed"]; c[2] += 1
        if arm["passed"] and not base["passed"]:
            rescues += 1; rescued_cases.add(base["case"])
        if base["passed"] and not arm["passed"]:
            breaks += 1
        extra = arm["tokens"] - base["tokens"]
        extra_tokens += extra
        extra_cost += (arm.get("costUsd") or 0) - (base.get("costUsd") or 0)
        if not base["rescue"]["testsFailed"] and extra:
            extra_untriggered += 1
    return dict(rescues=rescues, breaks=breaks, invalid=invalid, errors=errors,
                extra_tokens=extra_tokens, extra_cost=extra_cost, cases=rescued_cases,
                untriggered=extra_untriggered, per_case=per_case)


def bootstrap(per_case, n=10000, seed=7):
    """95 % interval of (arm − base) pass rate, resampling cases."""
    rng = random.Random(seed)
    keys = [k for k, v in per_case.items() if v[2]]
    if not keys:
        return (0.0, 0.0)
    diffs = []
    for _ in range(n):
        pick = [per_case[rng.choice(keys)] for _ in keys]
        runs_ = sum(p[2] for p in pick)
        diffs.append((sum(p[0] for p in pick) - sum(p[1] for p in pick)) / runs_)
    diffs.sort()
    return diffs[int(0.025 * n)], diffs[int(0.975 * n) - 1]


base_pass = sum(a["base"]["passed"] for a in valid)
print(f"{'brazo':<8} {'pasaron':>9} {'Δ vs base':>10} {'IC 95 %':>17} {'rescates':>9} {'roturas':>8} {'inválidos':>10} {'tokens extra':>13} {'$ extra':>9} {'tokens/rescate':>15}")
results = {}
for name in ARMS:
    if name == "base":
        print(f"{'base':<8} {base_pass:>4}/{len(valid):<4}")
        continue
    r = compare(name); results[name] = r
    n = sum(v[2] for v in r["per_case"].values())
    passed = sum(v[0] for v in r["per_case"].values())
    base_n = sum(v[1] for v in r["per_case"].values())
    lo, hi = bootstrap(r["per_case"])
    per = f"{r['extra_tokens']/r['rescues']:,.0f}" if r["rescues"] else "—"
    print(f"{SHORT[name]:<8} {passed:>4}/{n:<4} {(passed-base_n)/max(n,1):>+9.1%} [{lo:+.1%}, {hi:+.1%}] {r['rescues']:>9} {r['breaks']:>8} {r['invalid']:>10} {r['extra_tokens']:>13,} {r['extra_cost']:>9.4f} {per:>15}")

print("\npor caso (pasaron base / same / fresh / retry; disparos):")
for case in cases:
    rows = [a for a in valid if a["base"]["case"] == case]
    cells = [f"{sum(a[n]['passed'] for a in rows)}/{len(rows)}" for n in ARMS]
    fires = sum(a["base"]["rescue"]["testsFailed"] for a in rows)
    print(f"  {case:<22} " + "  ".join(f"{c:>5}" for c in cells) + f"   {fires}")

print("\nregla (PLAN.md): rescata ≥2 corridas en ≥2 casos, no rompe ninguna, rescata más que retry, sin tokens extra cuando no dispara")
retry = results["base+rescue:retry"]["rescues"]
for name in ("base+rescue:same", "base+rescue:fresh"):
    r = results[name]
    checks = [r["rescues"] >= 2 and len(r["cases"]) >= 2, r["breaks"] == 0, r["rescues"] > retry, r["untriggered"] == 0]
    print(f"  {SHORT[name]:<6} {'✓' if all(checks) else '✗'}  ({', '.join('sí' if c else 'no' for c in checks)})")
if len(fired) < 0.05 * len(valid):
    print("  disparo < 5 %: la función no puede rendir en esta suite (PLAN.md)")
