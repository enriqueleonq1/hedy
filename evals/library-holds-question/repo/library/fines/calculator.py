"""How much a late return costs."""
from datetime import date

from library.calendar.closures import ClosureCalendar
from library.calendar.loader import load_calendar
from library.catalog.models import Item
from library.loans.due import due_date
from library.loans.models import Loan
from library.members.models import Member
from library.members.tiers import pays_fines
from library.policy.rules import FINE_PER_OPEN_DAY_CENTS, MAX_FINE_PER_LOAN_CENTS


def late_open_days(loan: Loan, item: Item, member: Member, returned_on: date, calendar: ClosureCalendar) -> int:
    """Days the branch was open between the due date and the return."""
    due = due_date(loan, item, member, calendar)
    if returned_on <= due:
        return 0
    return calendar.open_days_between(due, returned_on)


def fine_for(
    loan: Loan,
    item: Item,
    member: Member,
    returned_on: date,
    calendar: ClosureCalendar | None = None,
) -> int:
    """Fine in cents for returning `loan` on `returned_on`.

    Only days the lending branch was open count, at the rate of the item kind,
    up to MAX_FINE_PER_LOAN_CENTS. Exempt tiers pay nothing.
    """
    if not pays_fines(member.tier):
        return 0
    calendar = calendar or load_calendar(loan.branch)
    days = late_open_days(loan, item, member, returned_on, calendar)
    return min(days * FINE_PER_OPEN_DAY_CENTS[item.kind], MAX_FINE_PER_LOAN_CENTS)
