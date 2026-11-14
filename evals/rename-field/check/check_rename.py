"""No customer_phone left in the code."""
import pathlib
import sys

left = [
    str(p)
    for p in pathlib.Path(".").rglob("*.py")
    if ".hedy" not in p.parts and p.name != "check_rename.py" and "customer_phone" in p.read_text()
]
if left:
    print("Todavía usan customer_phone:", ", ".join(sorted(left)))
    sys.exit(1)
