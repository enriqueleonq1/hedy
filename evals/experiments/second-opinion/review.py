#!/usr/bin/env python3
"""Test B's reviews (PLAN.md): each diff of the material, each arm, each repeat.

A reviewer is Claude Code with a clean context (`claude -p`, no setting
sources, no MCP, no saved session) in a copy of the case's repo with the diff
applied, with read-only tools. A verifier (the `+verify` arms) gets one finding
at a time in a new session on its own copy, with Bash as well.

    python3 review.py <account folder> <material.json> <out.jsonl> [--reps 3]
        [--models haiku,sonnet] [--jobs 6] [--only <item id>,...]

<account folder> is the Claude account's official folder (CLAUDE_CONFIG_DIR);
nothing in it is read here. Results already in <out.jsonl> are skipped, so a
stopped batch goes on where it was.
"""

import argparse, json, os, shutil, subprocess, tempfile, threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

SUITE = Path(__file__).resolve().parents[2]

FINDINGS = {
    "type": "object",
    "properties": {
        "findings": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "file": {"type": "string"},
                    "line": {"type": "integer"},
                    "claim": {"type": "string"},
                    "severity": {"type": "string", "enum": ["high", "medium", "low"]},
                    "confidence": {"type": "number"},
                },
                "required": ["file", "line", "claim", "severity", "confidence"],
            },
        }
    },
    "required": ["findings"],
}

VERDICT = {
    "type": "object",
    "properties": {"confirmed": {"type": "boolean"}, "reason": {"type": "string"}},
    "required": ["confirmed", "reason"],
}

# Variables that change the account or its billing never reach the CLI.
STRIP = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_BASE_URL"]


def context(item):
    case = json.loads((SUITE / item["case"] / "case.json").read_text())
    rules = [n["text"] for n in case.get("notes", []) + case.get("teamNotes", [])]
    text = f"Pedido original:\n{case['prompt']}\n"
    if rules:
        text += "\nReglas del proyecto:\n" + "".join(f"- {r}\n" for r in rules)
    diff = Path(item["patch"]).read_text()
    text += f"\nCambio (diff):\n```diff\n{diff[:30000]}\n```\n"
    return text


def review_prompt(item):
    return (
        "Revisa un cambio que hizo otra IA en este proyecto; la carpeta actual ya lo tiene aplicado.\n\n"
        + context(item)
        + "\nBusca errores reales: algo que falla o falla en un caso borde, rompe lo que ya funcionaba, "
        "no cumple el pedido o las reglas del proyecto, o expone datos. Puedes leer los archivos que "
        "necesites. No reportes estilo, nombres ni mejoras opcionales. Para cada error: archivo, línea, "
        "qué falla y en qué caso (claim), severidad (high: falla en uso normal o expone datos; medium: "
        "falla en un caso borde que el pedido o el proyecto contemplan; low: menor) y tu confianza de 0 a 1. "
        "Si no hay errores, devuelve findings vacío."
    )


def verify_prompt(item, finding):
    return (
        "Una revisión de este cambio dice que tiene este error:\n\n"
        f"{finding['file']}:{finding['line']} — {finding['claim']}\n\n"
        + context(item)
        + "\nCompruébalo en el código de la carpeta actual. Es una copia desechable: puedes correr código "
        "o escribir un script para reproducirlo. Responde confirmed true solo si lo reproduces o lo ves "
        "con certeza en el código; si no se sostiene, false. Explica en una línea (reason)."
    )


def checkout(item):
    """A throwaway copy of the case's repo with the diff applied."""
    folder = Path(tempfile.mkdtemp(prefix="hedy-review-"))
    shutil.copytree(SUITE / item["case"] / "repo", folder, dirs_exist_ok=True)
    git = lambda *a: subprocess.run(["git", *a], cwd=folder, check=True, capture_output=True)
    git("init", "-q", "-b", "main")
    git("-c", "user.name=r", "-c", "user.email=r@r.invalid", "add", "-A")
    git("-c", "user.name=r", "-c", "user.email=r@r.invalid", "commit", "-q", "-m", "base")
    if Path(item["patch"]).stat().st_size:
        git("apply", "--binary", str(Path(item["patch"]).resolve()))
    return folder


def claude(account, model, prompt, schema, tools, folder):
    env = {k: v for k, v in os.environ.items() if k not in STRIP}
    env["CLAUDE_CONFIG_DIR"] = account
    command = [
        "claude", "-p", prompt, "--model", model, "--setting-sources", "",
        "--strict-mcp-config", "--no-session-persistence", "--output-format", "json",
        "--json-schema", json.dumps(schema), "--tools", *tools, "--allowedTools", *tools,
    ]
    try:
        done = subprocess.run(command, cwd=folder, env=env, capture_output=True, text=True, timeout=600)
        out = json.loads(done.stdout)
    except (subprocess.TimeoutExpired, json.JSONDecodeError) as e:
        return {"error": f"{type(e).__name__}"}, 0.0, 0
    usage = out.get("usage", {})
    tokens = sum(usage.get(k, 0) for k in ("input_tokens", "output_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"))
    if out.get("is_error") or "structured_output" not in out:
        return {"error": str(out.get("result", ""))[:300]}, out.get("total_cost_usd", 0.0), tokens
    return out["structured_output"], out.get("total_cost_usd", 0.0), tokens


def one(account, item, model, rep):
    folder = checkout(item)
    try:
        answer, cost, tokens = claude(account, model, review_prompt(item), FINDINGS, ["Read", "Grep", "Glob"], folder)
    finally:
        shutil.rmtree(folder, ignore_errors=True)
    record = {"item": item["id"], "model": model, "rep": rep, "cost": cost, "tokens": tokens,
              "error": answer.get("error"), "findings": answer.get("findings", [])}
    # The verifier, for each finding that would be shown (high or medium).
    for finding in record["findings"]:
        if finding["severity"] not in ("high", "medium"):
            continue
        folder = checkout(item)
        try:
            verdict, cost, tokens = claude(account, model, verify_prompt(item, finding), VERDICT, ["Read", "Grep", "Glob", "Bash"], folder)
        finally:
            shutil.rmtree(folder, ignore_errors=True)
        finding["verify"] = {"confirmed": bool(verdict.get("confirmed")), "reason": verdict.get("reason", verdict.get("error", "")),
                             "cost": cost, "tokens": tokens, "error": verdict.get("error")}
    return record


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("account")
    parser.add_argument("material")
    parser.add_argument("out")
    parser.add_argument("--reps", type=int, default=3)
    parser.add_argument("--models", default="haiku,sonnet")
    parser.add_argument("--jobs", type=int, default=6)
    parser.add_argument("--only", default="")
    args = parser.parse_args()
    items = json.loads(Path(args.material).read_text())
    if args.only:
        items = [i for i in items if i["id"] in args.only.split(",")]
    done = set()
    if Path(args.out).exists():
        for line in open(args.out):
            r = json.loads(line)
            if not r.get("error"):
                done.add((r["item"], r["model"], r["rep"]))
    work = [(i, m, rep) for rep in range(1, args.reps + 1) for i in items for m in args.models.split(",")
            if (i["id"], m, rep) not in done]
    print(f"{len(work)} revisiones ({len(done)} ya hechas)", flush=True)
    lock = threading.Lock()

    def run(job):
        item, model, rep = job
        record = one(args.account, item, model, rep)
        with lock, open(args.out, "a") as out:
            out.write(json.dumps(record, ensure_ascii=False) + "\n")
        verified = sum(1 for f in record["findings"] if "verify" in f)
        print(f"· {item['id']} {model} #{rep}: {len(record['findings'])} hallazgos, {verified} verificados, ${record['cost']:.4f}{' · ' + record['error'] if record['error'] else ''}", flush=True)

    with ThreadPoolExecutor(args.jobs) as pool:
        list(pool.map(run, work))


if __name__ == "__main__":
    main()
