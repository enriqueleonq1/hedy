"""Monthly fines report, sent to the board on the first open day of each month."""
from dataclasses import dataclass
from datetime import date

from library.calendar.loader import load_calendar
from library.fines.calculator import fine_for, late_open_days
from library.money import fmt_cents
from library.state import Library
from library.vendor.textable import render


@dataclass(frozen=True)
class FineRow:
    member: str
    title: str
    returned: date
    late_days: int
    cents: int


def _in_month(day: date | None, year: int, month: int) -> bool:
    return day is not None and day.year == year and day.month == month


def monthly_fines(library: Library, year: int, month: int) -> list[FineRow]:
    """Fines of the loans returned in that month, by member and return date."""
    rows = []
    for loan in library.loans.values():
        if not _in_month(loan.returned, year, month):
            continue
        item = library.catalog.items[loan.item_id]
        member = library.members[loan.member_id]
        cents = fine_for(loan, item, member, loan.returned)
        if cents == 0:
            continue
        days = late_open_days(loan, item, member, loan.returned, load_calendar(loan.branch))
        title = library.catalog.titles[item.title_id].title
        rows.append(FineRow(member.name, title, loan.returned, days, cents))
    rows.sort(key=lambda r: (r.member, r.returned, r.title))
    return rows


def render_fines(rows: list[FineRow], year: int, month: int) -> str:
    heading = f"Multas de {month:02d}/{year}"
    if not rows:
        return f"{heading}\nSin multas."
    table = render(
        [[r.member, r.title, r.returned.strftime("%d/%m"), r.late_days, fmt_cents(r.cents)] for r in rows],
        headers=["Socio", "Título", "Devuelto", "Días", "Multa"],
        align="lllrr",
        footer=["Total", "", "", "", fmt_cents(sum(r.cents for r in rows))],
    )
    return f"{heading}\n{table}"
