"""How many loans each branch made in a month, by item kind."""
from collections import Counter

from library.catalog.models import KINDS
from library.state import Library
from library.vendor.textable import render

KIND_LABELS = {"book": "Libros", "dvd": "DVD", "magazine": "Revistas", "audiobook": "Audiolibros"}


def monthly_circulation(library: Library, year: int, month: int) -> dict[str, Counter]:
    by_branch: dict[str, Counter] = {}
    for loan in library.loans.values():
        if loan.checked_out.year != year or loan.checked_out.month != month:
            continue
        kind = library.catalog.items[loan.item_id].kind
        by_branch.setdefault(loan.branch, Counter())[kind] += 1
    return by_branch


def render_circulation(by_branch: dict[str, Counter], year: int, month: int) -> str:
    heading = f"Préstamos de {month:02d}/{year}"
    if not by_branch:
        return f"{heading}\nSin préstamos."
    rows = []
    for branch in sorted(by_branch):
        counts = by_branch[branch]
        rows.append([branch, *(counts.get(k, 0) for k in KINDS), sum(counts.values())])
    totals = [sum(r[i] for r in rows) for i in range(1, len(KINDS) + 2)]
    table = render(
        rows,
        headers=["Sucursal", *(KIND_LABELS[k] for k in KINDS), "Total"],
        align="l" + "r" * (len(KINDS) + 1),
        footer=["Total", *totals],
    )
    return f"{heading}\n{table}"
