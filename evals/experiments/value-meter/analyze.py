#!/usr/bin/env python3
"""Does the "value meter" tell right? (docs/decisions.md §50)

Reads hedy-evals results (JSON lines) of the value-meter experiment:
`full` (every step), `sin-<step>` (that step removed) and `none` (one agent).

1. The meter, as Hedy would compute it from the `full` runs only: a step
   "adds something" when, in some run, it sent the work back (a check that
   failed, a reviewer whose verdict asked for changes); it "adds nothing" when
   it never did. Nothing here knows the hidden checks.
2. The truth, from the ablations: removing a step that adds nothing should
   not lose cases (pass rate within the tolerance) and should save; removing
   one that adds something should lose cases.
3. The verdict: the share of "adds nothing" flags that were right.

    python3 analyze.py results.jsonl
"""

import json
import sys
from collections import defaultdict
from statistics import mean

# The full flow's steps: the check that decides each one (a reviewer counts
# through its verdict) and the ablation that removes it.
STEPS = {
    "tests": {"node": "s2", "label": "Tests"},
    "review": {"node": "s4", "label": "Revisor"},
    "review2": {"node": "s6", "label": "Revisor 2 (redundante)"},
    "security": {"node": "s8", "label": "Seguridad"},
    "rules": {"node": "s9", "label": "Reglas"},
}
# The share of runs a removed step may lose and still "add nothing" (one
# case out of six, at three repeats: about 1 run in 18).
TOLERANCE = 1 / 18
# The least saving that makes removing a step worth it.
MIN_SAVING = 0.10


def load(path):
    runs = []
    for line in open(path):
        r = json.loads(line)
        if r.get("error"):
            continue  # a run that never got to its check says nothing
        flow = r["variant"].split("+flow:")[1] if "+flow:" in r["variant"] else "none"
        runs.append({**r, "flow": flow})
    return runs


def meter(full_runs):
    """Per step: in how many runs it sent the work back."""
    sent_back = defaultdict(int)
    for r in full_runs:
        failed = {s["node"] for s in r.get("steps", []) if s["kind"] == "check" and not s["passed"]}
        for key, step in STEPS.items():
            if step["node"] in failed:
                sent_back[key] += 1
    return sent_back


def share_by_role(full_runs):
    """The share of the flow's tokens each role used."""
    totals = defaultdict(int)
    for r in full_runs:
        for s in r.get("steps", []):
            if s["kind"] == "role":
                totals[(s["node"], s["role"])] += s["tokens"]
    whole = sum(totals.values()) or 1
    return {k: v / whole for k, v in totals.items()}


def rate(runs):
    return sum(r["passed"] for r in runs) / len(runs) if runs else float("nan")


def main(path):
    runs = load(path)
    by_flow = defaultdict(list)
    for r in runs:
        by_flow[r["flow"]].append(r)
    full = by_flow.get("full", [])
    if not full:
        sys.exit("no runs of the full flow")
    cases = sorted({r["case"] for r in runs})
    print(f"Corridas válidas: {len(runs)} · casos: {len(cases)}\n")

    # What each variant does, overall and per case.
    # The paper's measure (arXiv 2608.01347): cost per solved task, on the
    # same cases for every variant (the cases every variant ran).
    names = ["none", "full"] + [f"sin-{k}" for k in STEPS]
    shared = set(cases)
    for name in names:
        if by_flow.get(name):
            shared &= {r["case"] for r in by_flow[name]}
    print(f"Por variante, en los {len(shared)} casos que corrieron todas")
    print("  variante        éxito      tokens/corrida   $/corrida   $ por tarea resuelta")
    for name in names:
        rs = [r for r in by_flow.get(name, []) if r["case"] in shared]
        if not rs:
            continue
        cost = sum(r.get("costUsd") or 0 for r in rs)
        solved = sum(r["passed"] for r in rs)
        per_solved = f"{cost / solved:.4f}" if solved else "—"
        print(
            f"  {name:<14} {rate(rs):>5.0%}  {mean(r['tokens'] for r in rs):>14,.0f}   "
            f"{cost / len(rs):>9.4f}   {per_solved:>10}  ({len(rs)} corridas)"
        )
    print()
    print("Por caso (éxitos / corridas)")
    print("  " + "caso".ljust(22) + "".join(n[:10].rjust(11) for n in names))
    for case in cases:
        row = []
        for name in names:
            rs = [r for r in by_flow.get(name, []) if r["case"] == case]
            row.append(f"{sum(r['passed'] for r in rs)}/{len(rs)}" if rs else "-")
        print("  " + case.ljust(22) + "".join(c.rjust(11) for c in row))
    print()

    # 1. The meter, from the full runs only.
    sent_back = meter(full)
    shares = share_by_role(full)
    print(f"El medidor (solo con las {len(full)} corridas del flujo completo)")
    flags = {}
    for key, step in STEPS.items():
        adds = sent_back[key] > 0
        flags[key] = adds
        print(f"  {step['label']:<24} devolvió el trabajo en {sent_back[key]:>2} corridas → {'APORTA' if adds else 'no aporta'}")
    print("  Gasto por rol:", ", ".join(f"{role}@{node} {v:.0%}" for (node, role), v in sorted(shares.items())))
    print()

    # 2. The truth, from the ablations: case by case, only the cases both
    # variants ran (averages over different cases compare apples and pears).
    print("La realidad (quitar cada paso; solo casos corridos con y sin él)")
    truth = {}
    for key, step in STEPS.items():
        rs = by_flow.get(f"sin-{key}", [])
        both = sorted({r["case"] for r in rs} & {r["case"] for r in full})
        if not both:
            continue
        with_rate = mean(rate([r for r in full if r["case"] == c]) for c in both)
        without_rate = mean(rate([r for r in rs if r["case"] == c]) for c in both)
        with_tokens = sum(mean(r["tokens"] for r in full if r["case"] == c) for c in both)
        without_tokens = sum(mean(r["tokens"] for r in rs if r["case"] == c) for c in both)
        lost = with_rate - without_rate
        saving = 1 - without_tokens / with_tokens
        useless = lost <= TOLERANCE and saving >= MIN_SAVING
        truth[key] = not useless
        print(
            f"  sin {step['label']:<22} {len(both)} casos · pasa {without_rate:>5.0%} contra {with_rate:>4.0%} "
            f"(pierde {lost:+.0%}) · ahorra {saving:>4.0%} → {'se puede quitar' if useless else 'hace falta'}"
        )
    print()

    # 3. The verdict.
    flagged = [k for k in truth if not flags[k]]
    right = [k for k in flagged if not truth[k]]
    missed = [k for k in truth if flags[k] and not truth[k]]
    print("Veredicto")
    if flagged:
        print(f"  De {len(flagged)} pasos marcados «no aporta», {len(right)} se podían quitar de verdad "
              f"({len(right) / len(flagged):.0%}; el criterio era ≥80%).")
        for k in flagged:
            print(f"   · {STEPS[k]['label']}: {'acierto' if k in right else 'ERROR: hacía falta'}")
    else:
        print("  El medidor no marcó ningún paso como «no aporta».")
    if missed:
        print("  Marcó «aporta» pasos que se podían quitar: " + ", ".join(STEPS[k]["label"] for k in missed))
    wrong = [k for k in flagged if k not in right]
    if not flagged:
        print("  → No hay evidencia a favor: el medidor no dijo nada útil.")
    elif len(right) / len(flagged) >= 0.8:
        print("  → APORTA VALOR: cuando dice «no aporta», casi siempre se puede quitar.")
    else:
        print(f"  → SE DESCARTA o se rediseña: se equivocó en {len(wrong)} de {len(flagged)}.")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "results.jsonl")
