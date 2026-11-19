#!/usr/bin/env python3
"""Eval of `hedy memory ask` with real tokens (docs/decisions.md §36): a fixed
memory of three projects and global notes, and questions with known answers. A
question passes when the answer has the expected words and cites a note that
has them, or, when nothing is written, when it says so.

Usage: scripts/ask-eval.py <claude-account-folder> [model]
It runs in a scratch data dir whose only account uses that folder (an account
you already logged in to with `claude`); it spends a few of its tokens."""
import json, os, shutil, sqlite3, subprocess, sys, time

if len(sys.argv) < 2:
    sys.exit(__doc__)
ACCOUNT = os.path.abspath(os.path.expanduser(sys.argv[1]))
MODEL = sys.argv[2] if len(sys.argv) > 2 else "haiku"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HEDY = os.path.join(ROOT, "target", "debug", "hedy")
import tempfile
BASE = tempfile.mkdtemp(prefix="hedy-ask-eval-")
ENV = dict(os.environ, HEDY_DATA_DIR=f"{BASE}/data", NO_COLOR="1", HEDY_LANGUAGE="es")

def h(*args, cwd=None, check=True):
    r = subprocess.run([HEDY, *args], cwd=cwd, env=ENV, capture_output=True, text=True)
    if check and r.returncode != 0:
        sys.exit(f"hedy {' '.join(args)} failed: {r.stderr}")
    return r

h("account", "add", "Eval", "--claude", "--no-login")
acc = f"{BASE}/data/accounts/eval"
shutil.rmtree(acc, ignore_errors=True)
os.symlink(ACCOUNT, acc)
h("account", "default", "eval")

projects = {}
for name in ["tienda", "agenda", "portal"]:
    path = f"{BASE}/code/{name}"
    os.makedirs(path)
    subprocess.run(["git", "init", "-q"], cwd=path)
    subprocess.run(["git", "remote", "add", "origin", f"https://github.com/ejemplo/{name}"], cwd=path)
    h("-C", path, "use", "eval")
    projects[name] = os.path.realpath(path)

NOTES = [
    (None, "Responde siempre en español y con frases cortas.", "preference"),
    (None, "Prefiero pnpm a npm en todos los proyectos.", "preference"),
    ("tienda", "El fondo oscuro de tienda es #0F1117 y el claro #FAFAF7.", "convention"),
    ("tienda", "Los precios se guardan en céntimos (enteros) y se muestran con dos decimales.", "decision"),
    ("tienda", "Los pagos usan Stripe; los webhooks se verifican con la firma antes de procesarlos.", "rule"),
    ("tienda", "Las migraciones de base de datos van en db/migrations y se escriben con sqlx.", "convention"),
    ("tienda", "Los tests de integración usan la base de datos de pruebas, nunca la de desarrollo.", "rule"),
    ("agenda", "Las citas se guardan en UTC y se muestran en la zona horaria del salón.", "decision"),
    ("agenda", "Los recordatorios se envían por WhatsApp 24 horas antes de la cita.", "decision"),
    ("agenda", "Si WhatsApp falla, el recordatorio se envía por SMS.", "decision"),
    ("agenda", "El frontend de agenda usa React con Vite y TanStack Query.", "convention"),
    ("portal", "En portal el cliente pidió fondo blanco puro #FFFFFF.", "decision"),
    ("portal", "Los textos de portal van en inglés británico.", "rule"),
    ("portal", "La API de portal devuelve snake_case; el cliente lo convierte a camelCase.", "convention"),
]
db = sqlite3.connect(f"{BASE}/data/hedy.db")
now = int(time.time())
for project, text, cat in NOTES:
    db.execute("INSERT INTO notes(project,text,category,author,status,created_at) VALUES(?,?,?,?,?,?)",
               (projects[project] if project else None, text, cat, "user", "approved", now))
db.commit()

# (scope args, cwd project, question, expected words in answer (any of each group), or None = not written)
CASES = [
    ([], "tienda", "¿Qué color de fondo usamos?", [["0F1117", "#0f1117"]]),
    ([], "tienda", "¿Cómo se guardan los precios?", [["céntimo", "centimo"]]),
    ([], "tienda", "¿Cómo verificamos los webhooks de pagos?", [["firma"]]),
    ([], "tienda", "¿Dónde van las migraciones?", [["db/migrations"]]),
    ([], "tienda", "¿Qué base de datos usan los tests de integración?", [["prueba"]]),
    ([], "tienda", "¿Qué tipografía usamos en los correos?", None),
    ([], "agenda", "¿En qué zona horaria se guardan las citas?", [["UTC"]]),
    ([], "agenda", "¿Qué pasa si falla WhatsApp?", [["SMS"]]),
    ([], "agenda", "¿Con cuánta antelación se envían los recordatorios?", [["24"]]),
    ([], "agenda", "¿Qué librería usamos para pedir datos en el frontend?", [["TanStack", "React Query"]]),
    (["--all"], None, "¿Qué fondo usa portal?", [["FFFFFF", "blanco"]]),
    (["--all"], None, "¿En qué idioma van los textos de portal?", [["británico", "britanico", "British"]]),
    (["--all"], None, "¿Qué gestor de paquetes prefiero?", [["pnpm"]]),
    (["--all"], None, "¿Qué proveedor de correo electrónico usamos?", None),
    (["--all"], None, "How does the portal API name its fields?", [["snake_case", "snake case"]]),
    (["--all"], None, "¿Cuál es el fondo de tienda y cuál el de portal?", [["0F1117", "#0f1117"], ["FFFFFF", "blanco"]]),
]

results = []
for scope, project, question, expected in CASES:
    cwd = projects[project] if project else BASE
    start = time.time()
    r = h("-C", cwd, "memory", "ask", "--json", "--model", MODEL, *scope, question, check=False)
    elapsed = time.time() - start
    try:
        data = json.loads(r.stdout)
    except json.JSONDecodeError:
        results.append((question, False, f"no JSON: {r.stderr.strip()[:120]}", elapsed))
        continue
    answer = data.get("answer") or ""
    cited = [s["text"] for s in data["sources"] if s.get("cited")]
    if expected is None:
        ok = data.get("answer") is None
        why = "dijo que no está escrito" if ok else f"respondió: {answer[:90]}"
    else:
        low = answer.lower()
        found = all(any(w.lower() in low for w in group) for group in expected)
        backed = all(any(any(w.lower() in c.lower() for w in group) for c in cited) for group in expected)
        ok = found and backed
        why = "ok" if ok else f"respuesta={found} cita={backed}: {answer[:90]}"
    results.append((question, ok, why, elapsed))

passed = sum(1 for _, ok, _, _ in results if ok)
for question, ok, why, elapsed in results:
    print(f"{'✔' if ok else '✗'} {elapsed:4.1f}s  {question}  — {why}")
times = sorted(e for *_, e in results)
print(f"\n{passed}/{len(results)} bien · mediana {times[len(times)//2]:.1f}s · máx {times[-1]:.1f}s")
shutil.rmtree(BASE, ignore_errors=True)
sys.exit(0 if passed == len(results) else 1)
