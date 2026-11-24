#!/usr/bin/env python3
"""Test B's analysis (PLAN.md): each arm's detections, false alarms and cost.

    python3 analyze.py <material.json> <reviews.jsonl> <labels.json> [<judge labels.json>]

<labels.json> are the hand labels ({finding key: {"label": cause|real|false}});
the judge's, if given, are compared with them (agreement and Cohen's kappa).
"""

import json, random, sys
from collections import defaultdict
from itertools import combinations
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from judge import key  # noqa: E402

material = {i["id"]: i for i in json.loads(Path(sys.argv[1]).read_text())}
reviews = [json.loads(l) for l in open(sys.argv[2]) if l.strip()]
labels = json.loads(Path(sys.argv[3]).read_text())
judged = json.loads(Path(sys.argv[4]).read_text()) if len(sys.argv) > 4 else None

by = {(r["item"], r["model"], r["rep"]): r for r in reviews if not r.get("error")}
reps = sorted({r["rep"] for r in reviews})
bad = [i for i, m in material.items() if m["bad"]]
good = [i for i, m in material.items() if not m["bad"]]


def shown(review, verify):
    """The findings a user would see: high or medium, confirmed if `verify`."""
    out = []
    for f in review["findings"]:
        if f["severity"] not in ("high", "medium"):
            continue
        if verify and not f.get("verify", {}).get("confirmed"):
            continue
        out.append(key(review["item"], f))
    return out


def cost(review, verify):
    extra = sum(f.get("verify", {}).get("cost", 0) for f in review["findings"]) if verify else 0
    return review["cost"] + extra


# An arm, for one repeat: the reviews it joins, whether it verifies, its calls.
def arms():
    out = {}
    for model in ("haiku", "sonnet"):
        name = "same" if model == "haiku" else "other"
        out[name] = [([(model, r)], False) for r in reps]
        out[name + "+verify"] = [([(model, r)], True) for r in reps]
    out["same×2"] = [([("haiku", a), ("haiku", b)], False) for a, b in combinations(reps, 2)]
    out["same+other"] = [([("haiku", r), ("sonnet", r)], False) for r in reps]
    return out


def measure(parts, verify):
    """Per diff: detected (bad) or false alarms (good), and the cost."""
    found, alarms, real_on_good, total, spent, real, n = {}, {}, {}, 0, 0.0, 0, 0
    for item in material:
        keys, missing = set(), False
        for model, rep in parts:
            review = by.get((item, model, rep))
            if review is None:
                missing = True
                break
            keys.update(shown(review, verify))
            spent += cost(review, verify)
        if missing:
            continue
        got = [labels.get(k, {}).get("label") for k in keys]
        total += len(got)
        real += sum(1 for g in got if g in ("cause", "real"))
        if material[item]["bad"]:
            found[item] = "cause" in got
        else:
            alarms[item] = sum(1 for g in got if g == "false")
            real_on_good[item] = sum(1 for g in got if g == "real")
    return found, alarms, real_on_good, total, real, spent


def bootstrap(rows, n=10000, seed=7):
    """95 % interval of a rate over diffs, resampling cases."""
    by_case = defaultdict(list)
    for item, value in rows.items():
        by_case[material[item]["case"]].append(value)
    cases = list(by_case)
    if not cases:
        return 0, 0
    rng, values = random.Random(seed), []
    for _ in range(n):
        pick = [v for _ in cases for v in by_case[rng.choice(cases)]]
        values.append(sum(pick) / len(pick))
    values.sort()
    return values[int(0.025 * n)], values[int(0.975 * n) - 1]


unlabelled = {k for r in reviews for f in r["findings"] if f["severity"] in ("high", "medium")
              for k in [key(r["item"], f)] if k not in labels}
print(f"{len(bad)} malos, {len(good)} buenos; repeticiones {reps}; hallazgos sin etiqueta: {len(unlabelled)}\n")
print(f"{'brazo':<14} {'detecta':>8} {'IC 95 %':>15} {'falsas/bueno':>13} {'IC 95 %':>15} {'reales en buenos':>17} {'precisión':>10} {'$/diff':>8} {'$/detectado':>12}  regla")
detected_by = {}
for name, runs in arms().items():
    if not runs:
        continue
    det_rows, fa_rows = defaultdict(list), defaultdict(list)
    totals = reals = 0
    spent = 0.0
    real_good = 0
    for parts, verify in runs:
        found, alarms, real_on_good, total, real, money = measure(parts, verify)
        for i, v in found.items():
            det_rows[i].append(v)
        for i, v in alarms.items():
            fa_rows[i].append(v)
        real_good += sum(real_on_good.values())
        totals += total; reals += real; spent += money
        if name in ("same", "other"):
            detected_by.setdefault(name, []).append({i for i, v in found.items() if v})
    det = {i: sum(v) / len(v) for i, v in det_rows.items()}
    fa = {i: sum(v) / len(v) for i, v in fa_rows.items()}
    rate = sum(det.values()) / max(len(det), 1)
    alarms = sum(fa.values()) / max(len(fa), 1)
    lo, hi = bootstrap(det)
    flo, fhi = bootstrap(fa)
    reviewed = sum(1 for parts, _ in runs for i in material if all((i, m, r) in by for m, r in parts))
    per_diff = spent / max(reviewed, 1)
    detections = sum(sum(v) for v in det_rows.values())
    per_detection = f"{spent / detections:.4f}" if detections else "—"
    passes = rate >= 0.5 and alarms < 0.25
    print(f"{name:<14} {rate:>8.0%} [{lo:>4.0%}, {hi:>4.0%}] {alarms:>13.2f} [{flo:>4.2f}, {fhi:>4.2f}] {real_good/len(runs):>17.1f} "
          f"{reals/max(totals,1):>10.0%} {per_diff:>8.4f} {per_detection:>12}  {'✓' if passes else '✗'}")

print("\nsolapamiento (malos detectados, por repetición): same y other / solo same / solo other / ninguno")
for s, o in zip(detected_by.get("same", []), detected_by.get("other", [])):
    print(f"  {len(s & o):>3} / {len(s - o):>3} / {len(o - s):>3} / {len(set(bad) - s - o):>3}")

print("\npor caso (malos detectados de 3 repeticiones: same / other / same+verify / other+verify)")
for case in sorted({m["case"] for m in material.values()}):
    items = [i for i in bad if material[i]["case"] == case]
    if not items:
        continue
    cells = []
    for name in ("same", "other", "same+verify", "other+verify"):
        hits = 0
        for parts, verify in arms()[name]:
            found = measure(parts, verify)[0]
            hits += sum(found.get(i, False) for i in items)
        cells.append(f"{hits}/{len(items) * len(reps)}")
    print(f"  {case:<22} " + "  ".join(f"{c:>6}" for c in cells))

if judged:
    both = [k for k in labels if k in judged]
    agree = sum(labels[k]["label"] == judged[k]["label"] for k in both)
    cats = ("cause", "real", "false")
    po = agree / max(len(both), 1)
    pe = sum((sum(labels[k]["label"] == c for k in both) / len(both)) * (sum(judged[k]["label"] == c for k in both) / len(both)) for c in cats) if both else 0
    kappa = (po - pe) / (1 - pe) if pe < 1 else 1.0
    print(f"\njuez ciego frente a etiquetas a mano: {agree}/{len(both)} de acuerdo ({po:.0%}), kappa {kappa:.2f}")
