"""Receiving a copy back: charge the fine, then pass it to the next hold."""
from dataclasses import dataclass
from datetime import date

from library.fines.calculator import fine_for
from library.holds.allocation import allocate_returned_copy
from library.holds.models import Hold
from library.state import Library


class ReturnError(Exception):
    pass


@dataclass(frozen=True)
class ReturnOutcome:
    loan_id: str
    fine_cents: int
    hold: Hold | None


def process_return(library: Library, loan_id: str, branch: str, on: date) -> ReturnOutcome:
    """Closes the loan, records its fine (if any) and allocates the copy."""
    loan = library.loans.get(loan_id)
    if loan is None or not loan.active:
        raise ReturnError(f"no active loan {loan_id}")
    if on < loan.checked_out:
        raise ReturnError("returned before it was lent")

    item = library.catalog.items[loan.item_id]
    member = library.members[loan.member_id]
    cents = fine_for(loan, item, member, on)
    library.ledger.charge(member.id, loan.id, cents, on)

    loan.returned = on
    loan.returned_at = branch
    hold = allocate_returned_copy(library, item, branch, on)
    return ReturnOutcome(loan.id, cents, hold)
