"""When a loan has to come back."""
from datetime import date, timedelta

from library.calendar.closures import ClosureCalendar
from library.catalog.models import Item
from library.members.models import Member
from library.members.tiers import extra_days
from library.policy.rules import LOAN_DAYS

from .models import Loan


def loan_period(item: Item, member: Member) -> int:
    """Days of one loan period for this item and member."""
    return LOAN_DAYS[item.kind] + extra_days(member.tier)


def due_date(loan: Loan, item: Item, member: Member, calendar: ClosureCalendar) -> date:
    """Last day to return without a fine.

    Each renewal adds one full period. A due date that falls on a day the branch
    is closed moves to the next open day.
    """
    days = loan_period(item, member) * (1 + loan.renewals)
    return calendar.next_open_day(loan.checked_out + timedelta(days=days))


def is_overdue(loan: Loan, item: Item, member: Member, calendar: ClosureCalendar, on: date) -> bool:
    if not loan.active:
        return False
    return on > due_date(loan, item, member, calendar)
