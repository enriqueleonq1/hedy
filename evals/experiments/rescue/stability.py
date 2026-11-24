#!/usr/bin/env python3
"""Before the rescue test: are a case's checks stable?

For each case with `tests` in its case.json, runs its visible tests and its
hidden check three times on the starting repo and on the reference solution
(repo/ with reference/ on top). A case is kept when every result repeats, the
hidden check fails on the start and both pass on the reference.

    python3 stability.py [evals folder] [runs]
"""

import json, shutil, subprocess, sys, tempfile
from pathlib import Path

suite = Path(sys.argv[1] if len(sys.argv) > 1 else "evals")
runs = int(sys.argv[2]) if len(sys.argv) > 2 else 3


def run(folder, command):
    done = subprocess.run(["sh", "-c", command], cwd=folder, capture_output=True, timeout=300)
    return done.returncode == 0


def tree(case, with_reference):
    folder = Path(tempfile.mkdtemp(prefix="hedy-stability-"))
    shutil.copytree(case / "repo", folder, dirs_exist_ok=True)
    if with_reference:
        shutil.copytree(case / "reference", folder, dirs_exist_ok=True)
    return folder


def results(case, spec, with_reference):
    seen = {"tests": [], "hidden": []}
    for _ in range(runs):
        # Visible tests on a tree without the hidden files, the hidden check on one with them.
        visible = tree(case, with_reference)
        seen["tests"].append(run(visible, spec["tests"]))
        shutil.rmtree(visible)
        hidden = tree(case, with_reference)
        shutil.copytree(case / "check", hidden, dirs_exist_ok=True)
        seen["hidden"].append(run(hidden, spec["check"]))
        shutil.rmtree(hidden)
    return seen


def show(values):
    return "".join("✓" if v else "✗" for v in values)


print(f"{'caso':<22} {'original tests':>15} {'original oculto':>16} {'ref. tests':>11} {'ref. oculto':>12}  ¿se queda?")
for case in sorted(p.parent for p in suite.glob("*/case.json")):
    spec = json.loads((case / "case.json").read_text())
    if not spec.get("tests") or not (case / "reference").is_dir():
        continue
    start, ref = results(case, spec, False), results(case, spec, True)
    stable = all(len(set(v)) == 1 for side in (start, ref) for v in side.values())
    keep = stable and not any(start["hidden"]) and all(ref["tests"]) and all(ref["hidden"])
    print(f"{case.name:<22} {show(start['tests']):>15} {show(start['hidden']):>16} {show(ref['tests']):>11} {show(ref['hidden']):>12}  {'sí' if keep else 'NO'}")
