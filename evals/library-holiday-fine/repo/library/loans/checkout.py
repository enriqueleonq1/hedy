"""Lending a copy at the front desk."""
from datetime import date

from library.members.standing import standing
from library.state import Library

from .models import Loan


class CheckoutError(Exception):
    """`reason` is a key of notifications.messages.CHECKOUT_REFUSALS."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


def checkout(library: Library, member_id: str, item_id: str, branch: str, on: date) -> Loan:
    member = library.members.get(member_id)
    if member is None:
        raise CheckoutError("unknown_member")
    item = library.catalog.items.get(item_id)
    if item is None:
        raise CheckoutError("unknown_item")
    if item_id in library.lent_item_ids():
        raise CheckoutError("already_lent")
    _check_reserved_for_someone_else(library, item_id, member_id)

    state = standing(member, library.ledger, active_loans=len(library.active_loans(member_id)))
    if not state.ok:
        raise CheckoutError(state.reason)

    loan = Loan(
        id=library.next_loan_id(),
        item_id=item_id,
        member_id=member_id,
        branch=branch,
        checked_out=on,
    )
    library.loans[loan.id] = loan
    _collect_own_hold(library, item_id, member_id)
    return loan


def _check_reserved_for_someone_else(library: Library, item_id: str, member_id: str) -> None:
    for hold in library.holds:
        if hold.status == "ready" and hold.item_id == item_id and hold.member_id != member_id:
            raise CheckoutError("reserved")


def _collect_own_hold(library: Library, item_id: str, member_id: str) -> None:
    for hold in library.holds:
        if hold.status == "ready" and hold.item_id == item_id and hold.member_id == member_id:
            hold.status = "collected"
