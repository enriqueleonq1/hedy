"""Everything the library knows at one moment, kept in memory."""
from dataclasses import dataclass, field

from library.catalog.snapshot import Catalog
from library.fines.ledger import FineLedger
from library.holds.models import Hold
from library.loans.models import Loan
from library.members.models import Member


@dataclass
class Library:
    catalog: Catalog
    members: dict[str, Member]
    loans: dict[str, Loan]
    holds: list[Hold] = field(default_factory=list)
    ledger: FineLedger = field(default_factory=FineLedger)

    def active_loans(self, member_id: str | None = None) -> list[Loan]:
        return [
            loan
            for loan in self.loans.values()
            if loan.active and (member_id is None or loan.member_id == member_id)
        ]

    def lent_item_ids(self) -> set[str]:
        return {loan.item_id for loan in self.loans.values() if loan.active}

    def next_loan_id(self) -> str:
        numbers = [int(i[1:]) for i in self.loans if i[1:].isdigit()]
        return f"L{max(numbers, default=0) + 1:05d}"

    def next_hold_id(self) -> str:
        numbers = [int(h.id[1:]) for h in self.holds if h.id[1:].isdigit()]
        return f"H{max(numbers, default=0) + 1:04d}"
