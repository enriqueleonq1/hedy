#!/usr/bin/env python3
"""The confirmation test's analysis (PLAN.md): each arm against hedy-hoy and the document.

Reads `hedy-evals --long … --sessions` results (JSON lines, several files). A run
is one case, arm, repeat and start; its sessions are its lines. Runs under the
third-party layer's account are the `layer` arm. A run with an error in any
session was voided (a usage limit, mostly) and run again: it is counted, not used.

    python3 analyze.py <evals/confirm> results.jsonl [more.jsonl ...] [--layer-account layer] [--pilot imapclient]
"""

import json, random, sys
from collections import defaultdict
from pathlib import Path

ARMS = ("today", "request", "doc", "layer", "flow")
NAMES = {"today": "hedy-hoy", "request": "pedido-repetido", "doc": "documento",
         "layer": "capa-de-terceros", "flow": "flujo"}

args = sys.argv[1:]
layer_account, pilot = "layer", "imapclient"
if "--layer-account" in args:
    i = args.index("--layer-account"); layer_account = args[i + 1]; del args[i:i + 2]
if "--pilot" in args:
    i = args.index("--pilot"); pilot = args[i + 1]; del args[i:i + 2]
suite, paths = Path(args[0]), args[1:]

runs = defaultdict(dict)
for path in paths:
    for line in open(path):
        if not line.strip():
            continue
        r = json.loads(line)
        if r["case"] == pilot:
            continue
        arm = r["variant"].split("+long:")[1]
        if r["account"] == layer_account:
            arm = "layer"
        runs[(r["case"], arm, r.get("model"), r["attempt"], r["startedAt"])][r["long"]["session"]] = r


def weighted(r):
    """Tokens at input price, as the session budget counts them."""
    return r.get("inputTokens", 0) + 5 * r.get("outputTokens", 0) + 0.1 * r.get("cacheReadTokens", 0) + 1.25 * r.get("cacheWriteTokens", 0)


# A session cut at its budget has no price from the CLI: its model's median rate.
rates = defaultdict(list)
for sessions in runs.values():
    for r in sessions.values():
        if r.get("costUsd") and weighted(r):
            rates[r.get("model")].append(r["costUsd"] / weighted(r))
rate = {m: sorted(ks)[len(ks) // 2] for m, ks in rates.items()}


def cost(r):
    return r["costUsd"] if r.get("costUsd") is not None else weighted(r) * rate.get(r.get("model"), 0.0)


complete, voided = {}, defaultdict(int)
spent = defaultdict(float)
for key, sessions in runs.items():
    spent[key[2]] += sum(cost(s) for s in sessions.values())
    if sessions and not any(s.get("error") for s in sessions.values()) and all(s["tokens"] for s in sessions.values()):
        complete[key] = sessions
    else:
        voided[key[2]] += 1


def reqs(session):
    return {k: v for k, v in session["long"]["reqs"].items() if k != "CHG"}


def row(sessions):
    order = sorted(sessions)
    last = sessions[order[-1]]
    met = reqs(last)
    regressions = 0
    for a, b in zip(order, order[1:]):
        before, after = reqs(sessions[a]), reqs(sessions[b])
        regressions += sum(1 for k, ok in before.items() if ok and not after.get(k, False))
    return {
        "met": sum(met.values()) / max(len(met), 1),
        "done": float(bool(met) and all(met.values()) and last["long"]["reqs"].get("CHG", False)),
        "change": float(last["long"]["reqs"].get("CHG", False)),
        "regressions": regressions,
        "cost": sum(cost(s) for s in sessions.values()),
        "tokens": sum(s["tokens"] for s in sessions.values()),
        "sessions": len(order),
        "first_cut": sessions[order[0]]["long"]["stopped"],
    }


table = defaultdict(lambda: defaultdict(list))  # (model, arm) -> case -> rows
for (case, arm, model, _, _), sessions in complete.items():
    table[(model, arm)][case].append(row(sessions))


def mean(xs):
    xs = [x for x in xs if x is not None]
    return sum(xs) / len(xs) if xs else float("nan")


def case_mean(rows, key):
    return mean([r[key] for r in rows])


def boot(arm, base, key="met", n=10000, seed=7):
    """Paired by case: the mean over cases of (arm − base), its 95 % interval resampling cases."""
    cases = sorted(c for c in arm if c in base)
    if not cases:
        return float("nan"), float("nan"), float("nan")
    diffs = [case_mean(arm[c], key) - case_mean(base[c], key) for c in cases]
    rng, out = random.Random(seed), []
    for _ in range(n):
        out.append(mean([rng.choice(diffs) for _ in diffs]))
    out.sort()
    return mean(diffs), out[int(0.025 * n)], out[int(0.975 * n) - 1]


def pct(x):
    return "—" if x != x else f"{x * 100:+.0f}"


print(f"{len(complete)} corridas completas; anuladas y repetidas (límite de uso u error): "
      + ", ".join(f"{m} {n}" for m, n in sorted(voided.items(), key=str)) if voided else f"{len(complete)} corridas completas, ninguna anulada")
for model, k in sorted(rate.items(), key=str):
    print(f"  {model}: $ {k * 1e6:.3f} por millón de tokens a precio de entrada (mediana); gastado $ {spent[model]:.2f}")
print()

for model in sorted({m for m, _ in table}, key=str):
    today, doc = table.get((model, "today"), {}), table.get((model, "doc"), {})
    # Rule: a case Sonnet finished in one session in most runs is left out.
    cut = defaultdict(list)
    for (m, arm), cases in table.items():
        if m == model:
            for c, rs in cases.items():
                cut[c] += [r["first_cut"] for r in rs]
    out = sorted(c for c, xs in cut.items() if sum(xs) * 2 < len(xs))
    if out:
        print(f"== {model}: casos fuera (su primera sesión terminó sin corte en la mayoría): {', '.join(out)}")
        for cases in (t for (m, _), t in table.items() if m == model):
            for c in out:
                cases.pop(c, None)
    print(f"== {model}")
    print(f"{'brazo':<17} {'n':>3} {'req.':>5} {'Δ vs hoy':>9} {'IC 95 %':>13} {'Δ vs doc':>9} {'IC 95 %':>13} "
          f"{'termina':>8} {'cambio':>7} {'regr.':>6} {'ses.':>5} {'$/corr.':>8} {'× hoy':>6}")
    base_cost = mean([case_mean(rs, "cost") for rs in today.values()])
    for arm in ARMS:
        cases = table.get((model, arm))
        if not cases:
            continue
        rows = [r for rs in cases.values() for r in rs]
        d1, l1, h1 = boot(cases, today) if arm != "today" else (float("nan"),) * 3
        d2, l2, h2 = boot(cases, doc) if arm != "doc" else (float("nan"),) * 3
        c = mean([case_mean(rs, "cost") for rs in cases.values()])
        print(f"{NAMES[arm]:<17} {len(rows):>3} {mean([case_mean(rs, 'met') for rs in cases.values()]):>5.0%} "
              f"{pct(d1):>9} {('[' + pct(l1) + ', ' + pct(h1) + ']') if d1 == d1 else '':>13} "
              f"{pct(d2):>9} {('[' + pct(l2) + ', ' + pct(h2) + ']') if d2 == d2 else '':>13} "
              f"{mean([r['done'] for r in rows]):>8.0%} {mean([r['change'] for r in rows]):>7.0%} "
              f"{mean([r['regressions'] for r in rows]):>6.2f} {mean([r['sessions'] for r in rows]):>5.1f} "
              f"{c:>8.3f} {c / base_cost if base_cost else float('nan'):>6.2f}")
    print("\n  por caso (requisitos tras la última sesión, media; corridas):")
    print(f"    {'caso':<16} " + " ".join(f"{NAMES[a][:10]:>11}" for a in ARMS))
    for case in sorted({c for (m, _), t in table.items() if m == model for c in t}):
        cells = []
        for arm in ARMS:
            rs = table.get((model, arm), {}).get(case, [])
            cells.append(f"{case_mean(rs, 'met'):>7.0%} ({len(rs)})" if rs else f"{'—':>11}")
        print(f"    {case:<16} " + " ".join(cells))
    # The rules of PLAN.md, as fixed.
    if today and doc:
        d, lo, hi = boot(doc, today)
        dr, lor, hir = boot(doc, today, "regressions")
        ratio = mean([case_mean(rs, "cost") for rs in doc.values()]) / base_cost
        print("\n  reglas:")
        print(f"    documento confirmado (≥ +10, IC inf. > 0, coste ≤ 1,2×, regresiones ≤ hoy): "
              f"Δ {pct(d)} [{pct(lo)}, {pct(hi)}], coste {ratio:.2f}×, Δ regresiones {dr:+.2f} → "
              f"{'cumple' if d >= 0.10 and lo > 0 and ratio <= 1.2 and dr <= 0 else 'no cumple'}")
        request = table.get((model, "request"), {})
        if request:
            dd, lo2, hi2 = boot(doc, request)
            print(f"    documento ≈ pedido-repetido (la diferencia cabe en su IC: incluye 0): Δ {pct(dd)} [{pct(lo2)}, {pct(hi2)}] → "
                  f"{'sí: versión mínima primero' if lo2 <= 0 <= hi2 else 'no'}")
        for arm in ("layer", "flow"):
            other = table.get((model, arm), {})
            if other:
                da, la, ha = boot(other, doc)
                ca = mean([case_mean(rs, "cost") for rs in other.values()]) / mean([case_mean(rs, "cost") for rs in doc.values()])
                verdict = ("gana al documento: NO se borran los flujos" if da >= 0.10 and la > 0
                           else "no gana al documento")
                print(f"    {NAMES[arm]} vs documento: Δ {pct(da)} [{pct(la)}, {pct(ha)}], coste {ca:.2f}× el documento → {verdict}")
        print(f"    hedy-hoy a ≤ 5 puntos del documento: {'sí: el documento no es prioridad' if d <= 0.05 else 'no'}")
    print()
