"""Days the library is closed: every Sunday and the public holidays."""
from datetime import date, timedelta

SUNDAY = 6

# A calendar with every day closed would loop forever; nothing closes this long.
_MAX_CLOSED_STRETCH = 60


class ClosureCalendar:
    """Answers whether a branch is open on a given day.

    `holidays` holds `datetime.date` values.
    """

    def __init__(self, holidays=(), closed_weekdays=(SUNDAY,)):
        self.holidays = set(holidays)
        self.closed_weekdays = frozenset(closed_weekdays)

    def is_open(self, day: date) -> bool:
        return day.weekday() not in self.closed_weekdays and day not in self.holidays

    def next_open_day(self, day: date) -> date:
        """`day` itself when the library is open, otherwise the first open day after it."""
        for _ in range(_MAX_CLOSED_STRETCH):
            if self.is_open(day):
                return day
            day += timedelta(days=1)
        raise ValueError(f"no open day within {_MAX_CLOSED_STRETCH} days of {day}")

    def add_open_days(self, day: date, count: int) -> date:
        """The `count`-th open day after `day`."""
        while count > 0:
            day += timedelta(days=1)
            if self.is_open(day):
                count -= 1
        return day

    def open_days_between(self, start: date, end: date) -> int:
        """Open days after `start`, up to and including `end`."""
        count = 0
        day = start + timedelta(days=1)
        while day <= end:
            if self.is_open(day):
                count += 1
            day += timedelta(days=1)
        return count
