#!/usr/bin/env python3
"""Planted errors for test B (PLAN.md): a reference solution with one mistake.

Each seed is the case's reference solution with one edit. It is kept only if
the project's tests pass and the hidden check fails; its patch (against the
case's repo) goes to seeds/<id>.patch, and seeds.json lists them with the
real cause, for labelling.

    python3 make_seeds.py
"""

import json, shutil, subprocess, tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SUITE = HERE.parents[1]

# id, case, the real cause, [(file, old, new)]
SEEDS = [
    ("inventory-boundary", "inventory-expiry",
     "a reservation of exactly 15 minutes is not expired yet (> instead of >=)",
     [("shop/inventory.py", ">= HOLD_SECONDS", "> HOLD_SECONDS")]),
    ("limiter-boundary", "rate-limit-bugs",
     "a client that waits exactly the whole window is still blocked (< instead of <=)",
     [("limiter/sliding.py", "hits[0] <= cutoff", "hits[0] < cutoff")]),
    ("attachment-absolute", "attachment-download",
     "only '..' is refused: an absolute or symlinked name escapes the upload folder",
     [("app/routes.py",
       "    target = (root / name).resolve()\n    if target.parent != root or not target.is_file():",
       "    target = root / name\n    if \"..\" in name.split(\"/\") or not target.is_file():")]),
    ("shipping-threshold", "shipping-refactor",
     "international shipping becomes free at exactly 100 (>= instead of >)",
     [("store/shipping.py", "subtotal > 100", "subtotal >= 100")]),
    ("receipt-key-default", "receipt-api-key",
     "the test key is written in the code as the default of MESSAGING_API_KEY",
     [("billing/receipts.py", 'os.environ.get("MESSAGING_API_KEY")',
       'os.environ.get("MESSAGING_API_KEY", "mk_test_4eC39HqLyjWDarjtT1zdp7dc")')]),
    ("summary-print", "sales-summary-rules",
     "the summary is also printed to the console, against the rule that reports/ never prints",
     [("reports/summary.py", '    logger.info("Ventas de %s: %s", month, total)',
       '    logger.info("Ventas de %s: %s", month, total)\n    print(f"Ventas de {month}: {total}")')]),
    ("greeting-punctuation", "easy-greeting",
     "the greeting lacks the closing '!'",
     [("app/greetings.py", 'f"Bienvenido, {name}!"', 'f"Bienvenido, {name}"')]),
    ("business-single-day", "business-days",
     "a one-day range always counts 1, even on a weekend",
     [("calendar_utils.py", "    days = 0\n", "    if start == end:\n        return 1\n    days = 0\n")]),
    ("duration-bare-number", "parse-duration",
     "a bare number ('30', '1h30') is taken as minutes instead of refused",
     [("timer/durations.py", r'(?:(\d+)m)?")', r'(?:(\d+)m?)?")')]),
    ("summary-float", "sales-summary-rules",
     "the month's total is added as float, against the rule that money is Decimal",
     [("reports/summary.py",
       "    total = sum(\n        (Decimal(str(s[\"amount\"])) for s in sales if s[\"date\"].startswith(f\"{month}-\")),\n        Decimal(\"0\"),\n    )",
       "    total = Decimal(sum(float(s[\"amount\"]) for s in sales if s[\"date\"].startswith(f\"{month}-\")))")]),
    ("inventory-counts-expired", "inventory-expiry",
     "expired reservations still hold stock until release_expired() runs",
     [("shop/inventory.py",
       "return sum(r.qty for r in self._reservations.values() if r.sku == sku and self.active(r))",
       "return sum(r.qty for r in self._reservations.values() if r.sku == sku and not r.confirmed)")]),
    ("business-saturday", "business-days",
     "Saturday counts as a business day (weekday() < 6)",
     [("calendar_utils.py", "if current.weekday() < 5:", "if current.weekday() < 6:")]),
    ("receipt-env-name", "receipt-api-key",
     "the key is read from MESSAGING_KEY, not MESSAGING_API_KEY as docs/servicio.md says",
     [("billing/receipts.py", '"MESSAGING_API_KEY"', '"MESSAGING_KEY"'),
      ("tests/test_receipts.py", '"MESSAGING_API_KEY"', '"MESSAGING_KEY"')]),
    ("cents-truncate", "billing-cents",
     "tax and ratios are truncated to the cent instead of rounded half up",
     [("billing/money.py", "return (amount * num * 2 + den) // (2 * den)", "return amount * num // den")]),
    ("bakery-per-line", "bakery-inscription",
     "the inscription is charged once per product line instead of once per order",
     [("bakery/pricing.py",
       '        lines.append((label, line.qty * p.price_cents))\n',
       '        lines.append((label, line.qty * p.price_cents))\n        if order.inscription is not None:\n            lines.append(("Dedicatoria", EXTRAS_CENTS["inscription"]))\n'),
      ("bakery/pricing.py", '    ("Dedicatoria", "inscription", lambda o: o.inscription is not None),\n', "")]),
]


def sh(folder, command):
    return subprocess.run(["sh", "-c", command], cwd=folder, capture_output=True, text=True, timeout=300)


def build(case, edits):
    folder = Path(tempfile.mkdtemp(prefix="hedy-seed-"))
    shutil.copytree(SUITE / case / "repo", folder, dirs_exist_ok=True)
    sh(folder, "git init -q -b main && git add -A && git -c user.name=s -c user.email=s@s.invalid commit -qm base")
    shutil.copytree(SUITE / case / "reference", folder, dirs_exist_ok=True)
    for file, old, new in edits:
        path = folder / file
        text = path.read_text()
        if text.count(old) != 1:
            raise SystemExit(f"{case}: {old!r} is not once in {file}")
        path.write_text(text.replace(old, new))
    return folder


def main():
    out = HERE / "seeds"
    out.mkdir(exist_ok=True)
    kept = []
    for seed, case, cause, edits in SEEDS:
        spec = json.loads((SUITE / case / "case.json").read_text())
        folder = build(case, edits)
        try:
            sh(folder, "git add -A")
            patch = sh(folder, "git diff --cached --binary HEAD -- . ':(exclude,glob)**/__pycache__/**'").stdout
            visible = sh(folder, spec["tests"]).returncode == 0
            shutil.copytree(SUITE / case / "check", folder, dirs_exist_ok=True)
            done = sh(folder, spec["check"])
            hidden = done.returncode == 0
        finally:
            shutil.rmtree(folder, ignore_errors=True)
        ok = visible and not hidden
        print(f"{seed:<24} tests {'✓' if visible else '✗'}  oculto {'✓' if hidden else '✗'}  {'se queda' if ok else 'NO'}")
        if ok:
            (out / f"{seed}.patch").write_text(patch)
            output = (done.stdout + done.stderr).replace(f"{folder}/", "").strip()[-2000:]
            kept.append({"id": f"seed:{seed}", "case": case, "cause": cause, "patch": f"seeds/{seed}.patch",
                         "checkOutput": output})
    (HERE / "seeds.json").write_text(json.dumps(kept, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
