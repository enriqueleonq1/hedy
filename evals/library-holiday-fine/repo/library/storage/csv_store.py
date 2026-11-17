"""Loads the library state from the CSV files in fixtures/.

The CSV files are the export of the front desk system; dates are ISO
(YYYY-MM-DD) and empty cells mean "not set".
"""
import csv
from datetime import date
from pathlib import Path

from library import config
from library.catalog.snapshot import load_snapshot
from library.fines.ledger import FineEntry, FineLedger
from library.holds.models import Hold
from library.loans.models import Loan
from library.members.models import Member
from library.state import Library


def _rows(path: Path) -> list[dict]:
    with open(path, encoding="utf-8", newline="") as f:
        return list(csv.DictReader(f))


def _date(text: str) -> date | None:
    text = text.strip()
    return date.fromisoformat(text) if text else None


def _bool(text: str) -> bool:
    return text.strip().lower() in {"1", "true", "yes", "si", "sí"}


def load_members(path: Path = config.MEMBERS_FILE) -> dict[str, Member]:
    members = {}
    for row in _rows(path):
        member = Member(
            id=row["id"],
            name=row["name"],
            tier=row["tier"],
            home_branch=row["home_branch"],
            email=row.get("email", ""),
            suspended=_bool(row.get("suspended", "")),
        )
        members[member.id] = member
    return members


def load_loans(path: Path = config.LOANS_FILE) -> dict[str, Loan]:
    loans = {}
    for row in _rows(path):
        loan = Loan(
            id=row["id"],
            item_id=row["item_id"],
            member_id=row["member_id"],
            branch=row["branch"],
            checked_out=_date(row["checked_out"]),
            returned=_date(row.get("returned", "")),
            returned_at=row.get("returned_at") or None,
            renewals=int(row.get("renewals") or 0),
        )
        loans[loan.id] = loan
    return loans


def load_holds(path: Path = config.HOLDS_FILE) -> list[Hold]:
    return [
        Hold(
            id=row["id"],
            member_id=row["member_id"],
            title_id=row["title_id"],
            pickup_branch=row["pickup_branch"],
            placed_on=_date(row["placed_on"]),
            status=row.get("status") or "waiting",
        )
        for row in _rows(path)
    ]


def load_ledger(path: Path = config.FINES_FILE) -> FineLedger:
    return FineLedger(
        [
            FineEntry(
                member_id=row["member_id"],
                loan_id=row["loan_id"],
                cents=int(row["cents"]),
                charged_on=_date(row["charged_on"]),
                paid=_bool(row.get("paid", "")),
            )
            for row in _rows(path)
        ]
    )


def load_library(fixtures: Path | None = None) -> Library:
    """The whole state, from `fixtures` (fixtures/ by default)."""
    base = Path(fixtures) if fixtures else config.FIXTURES_DIR
    return Library(
        catalog=load_snapshot(),
        members=load_members(base / "members.csv"),
        loans=load_loans(base / "loans.csv"),
        holds=load_holds(base / "holds.csv"),
        ledger=load_ledger(base / "fines.csv"),
    )
