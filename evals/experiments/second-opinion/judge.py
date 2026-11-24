#!/usr/bin/env python3
"""Test B's blind judge (PLAN.md): labels every high or medium finding.

One call per diff: the request, the project's rules, the diff, the hidden
check's verdict and output, and that diff's distinct findings from every arm
and repeat, shuffled and without saying whose they are. Each finding gets
`cause` (it names why the hidden check fails), `real` (a real defect, not that
one) or `false`.

    python3 judge.py <account folder> <material.json> <reviews.jsonl> <labels.json> [--model opus]
"""

import argparse, hashlib, json, random, sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from review import checkout, claude, context  # noqa: E402

LABELS = {
    "type": "object",
    "properties": {
        "labels": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "n": {"type": "integer"},
                    "label": {"type": "string", "enum": ["cause", "real", "false"]},
                    "note": {"type": "string"},
                },
                "required": ["n", "label", "note"],
            },
        }
    },
    "required": ["labels"],
}


def key(item, finding):
    text = f"{item}|{finding['file']}|{finding['line']}|{finding['claim']}"
    return hashlib.sha1(text.encode()).hexdigest()[:16]


def shown(reviews):
    """Each diff's distinct high or medium findings."""
    by = {}
    for r in reviews:
        for f in r["findings"]:
            if f["severity"] in ("high", "medium"):
                by.setdefault(r["item"], {})[key(r["item"], f)] = f
    return by


def prompt(item, findings):
    if item["bad"]:
        verdict = ("El cambio NO pasa las pruebas ocultas del proyecto. Su salida:\n```\n"
                   + item["checkOutput"][-2000:] + "\n```\n")
    else:
        verdict = "El cambio pasa todas las pruebas ocultas del proyecto.\n"
    listed = "".join(f"{n}. {f['file']}:{f['line']} — {f['claim']}\n" for n, f in enumerate(findings, 1))
    return (
        "Eres juez de revisiones de código. Varias revisiones de un mismo cambio hicieron estos "
        "hallazgos; no sabes de qué revisión viene cada uno.\n\n" + context(item) + "\n" + verdict
        + "\nHallazgos:\n" + listed
        + "\nEtiqueta cada uno: cause si nombra la razón por la que fallan las pruebas ocultas; real si "
        "es un defecto real del cambio (falla de verdad en algún caso que el pedido, las reglas o el "
        "proyecto contemplan) pero no esa razón; false si no es un defecto real (no falla, es estilo o "
        "preferencia, o malinterpreta el código). Puedes leer el código de la carpeta actual, que tiene "
        "el cambio aplicado. Una nota corta por hallazgo."
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("account")
    parser.add_argument("material")
    parser.add_argument("reviews")
    parser.add_argument("labels")
    parser.add_argument("--model", default="opus")
    parser.add_argument("--jobs", type=int, default=4)
    args = parser.parse_args()
    items = {i["id"]: i for i in json.loads(Path(args.material).read_text())}
    reviews = [json.loads(l) for l in open(args.reviews) if l.strip()]
    labels = json.loads(Path(args.labels).read_text()) if Path(args.labels).exists() else {}
    work = [(i, fs) for i, fs in shown(reviews).items() if any(k not in labels for k in fs)]
    print(f"{len(work)} diffs por juzgar", flush=True)
    rng = random.Random(7)

    def judge(job):
        item_id, findings = job
        keys = list(findings)
        rng.shuffle(keys)
        folder = checkout(items[item_id])
        try:
            answer, cost, _ = claude(args.account, args.model, prompt(items[item_id], [findings[k] for k in keys]),
                                     LABELS, ["Read", "Grep", "Glob"], folder)
        finally:
            import shutil
            shutil.rmtree(folder, ignore_errors=True)
        got = {}
        for label in answer.get("labels", []):
            if 1 <= label["n"] <= len(keys):
                got[keys[label["n"] - 1]] = {"label": label["label"], "note": label["note"], "item": item_id}
        print(f"· {item_id}: {len(got)}/{len(keys)} etiquetas, ${cost:.4f}{' · ' + answer['error'] if answer.get('error') else ''}", flush=True)
        return got, cost

    total = 0.0
    with ThreadPoolExecutor(args.jobs) as pool:
        for got, cost in pool.map(judge, work):
            labels.update(got)
            total += cost
    Path(args.labels).write_text(json.dumps(labels, ensure_ascii=False, indent=1) + "\n")
    print(f"juez: ${total:.4f}")


if __name__ == "__main__":
    main()
