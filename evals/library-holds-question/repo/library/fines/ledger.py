"""Fines already charged, paid or not."""
from dataclasses import dataclass, field
from datetime import date


@dataclass(frozen=True)
class FineEntry:
    member_id: str
    loan_id: str
    cents: int
    charged_on: date
    paid: bool = False


@dataclass
class FineLedger:
    entries: list[FineEntry] = field(default_factory=list)

    def charge(self, member_id: str, loan_id: str, cents: int, on: date) -> FineEntry | None:
        if cents <= 0:
            return None
        entry = FineEntry(member_id, loan_id, cents, on)
        self.entries.append(entry)
        return entry

    def unpaid_cents(self, member_id: str) -> int:
        return sum(e.cents for e in self.entries if e.member_id == member_id and not e.paid)

    def pay_all(self, member_id: str) -> int:
        """Marks every fine of the member as paid; returns what was owed."""
        owed = 0
        kept = []
        for e in self.entries:
            if e.member_id == member_id and not e.paid:
                owed += e.cents
                e = FineEntry(e.member_id, e.loan_id, e.cents, e.charged_on, paid=True)
            kept.append(e)
        self.entries = kept
        return owed
