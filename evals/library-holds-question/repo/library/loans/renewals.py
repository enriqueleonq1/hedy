"""Renewing a loan from the website or the front desk."""
from datetime import date

from library.calendar.loader import load_calendar
from library.holds.queue import waiting_holds
from library.policy.rules import MAX_RENEWALS
from library.state import Library

from .due import due_date
from .models import Loan


class RenewalError(Exception):
    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


def renew(library: Library, loan_id: str, on: date) -> Loan:
    """Adds one loan period, unless someone is waiting for the title.

    Overdue loans cannot be renewed: they have to come back first.
    """
    loan = library.loans.get(loan_id)
    if loan is None or not loan.active:
        raise RenewalError("unknown_loan")
    if loan.renewals >= MAX_RENEWALS:
        raise RenewalError("max_renewals")

    item = library.catalog.items[loan.item_id]
    member = library.members[loan.member_id]
    if on > due_date(loan, item, member, load_calendar(loan.branch)):
        raise RenewalError("overdue")
    if waiting_holds(library.holds, item.title_id):
        raise RenewalError("on_hold")

    loan.renewals += 1
    return loan
