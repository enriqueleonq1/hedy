"""Loans still out after their due date, for the reminder calls."""
from dataclasses import dataclass
from datetime import date

from library.calendar.loader import load_calendar
from library.loans.due import due_date, is_overdue
from library.state import Library
from library.vendor.textable import render


@dataclass(frozen=True)
class OverdueRow:
    member: str
    title: str
    branch: str
    due: date
    days: int


def overdue_on(library: Library, on: date) -> list[OverdueRow]:
    rows = []
    for loan in library.active_loans():
        item = library.catalog.items[loan.item_id]
        member = library.members[loan.member_id]
        calendar = load_calendar(loan.branch)
        if not is_overdue(loan, item, member, calendar, on):
            continue
        due = due_date(loan, item, member, calendar)
        title = library.catalog.titles[item.title_id].title
        rows.append(OverdueRow(member.name, title, loan.branch, due, (on - due).days))
    rows.sort(key=lambda r: (-r.days, r.member))
    return rows


def render_overdue(rows: list[OverdueRow], on: date) -> str:
    heading = f"Préstamos vencidos al {on.strftime('%d/%m/%Y')}"
    if not rows:
        return f"{heading}\nNinguno."
    table = render(
        [[r.member, r.title, r.branch, r.due.strftime("%d/%m"), r.days] for r in rows],
        headers=["Socio", "Título", "Sucursal", "Vencía", "Días"],
        align="llllr",
    )
    return f"{heading}\n{table}"
