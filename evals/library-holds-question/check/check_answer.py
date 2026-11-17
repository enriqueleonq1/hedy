"""The answer names the rule, where it lives and its limit; no code changed."""
import pathlib
import re
import subprocess
import sys
import unicodedata

errors = []


def fold(text: str) -> str:
    decomposed = unicodedata.normalize("NFKD", text)
    return "".join(c for c in decomposed if not unicodedata.combining(c)).lower()


def git(*args: str) -> str:
    return subprocess.run(["git", *args], capture_output=True, text=True, check=True).stdout


# No Python file changed, staged, committed or added since the case's first commit.
root = git("rev-list", "--max-parents=0", "HEAD").split()[0]
changed = [p for p in git("diff", "--name-only", root, "--", "*.py").split() if p]
added = [
    p
    for p in git("ls-files", "--others", "--exclude-standard", "--", "*.py").split()
    if pathlib.Path(p).name != pathlib.Path(__file__).name
]
if changed or added:
    errors.append("cambió código: " + ", ".join(sorted(changed + added)))

answers = [p for p in pathlib.Path(".").iterdir() if p.is_file() and p.name.lower() == "respuesta.md"]
if not answers:
    errors.append("no hay RESPUESTA.md en la raíz")
    text = ""
else:
    text = fold(answers[0].read_text(encoding="utf-8", errors="replace"))

required = {
    "dónde se decide (hold_sort_key / priority.py)": [r"hold_sort_key", r"priority\.py", r"holds[./]priority"],
    "la preferencia por la sucursal donde se devolvió": [
        r"\bcentro\b",
        r"misma sucursal",
        r"same branch",
        r"pickup_branch",
        r"returned_at",
    ],
    "el límite de 7 días": [
        r"local_preference_days",
        r"\b(7|siete|seven)\s*(dias|days)\b",
        r"mas de (7|siete)\b",
        r"more than (7|seven)\b",
        r"una semana",
    ],
}
for what, patterns in required.items():
    if text and not any(re.search(p, text) for p in patterns):
        errors.append(f"la respuesta no menciona {what}")

if errors:
    print("\n".join(errors))
    sys.exit(1)
print("ok")
