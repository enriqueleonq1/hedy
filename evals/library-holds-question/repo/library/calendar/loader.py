"""Builds each branch's closure calendar from data/holidays.json."""
import json
from datetime import date
from functools import lru_cache

from library.config import HOLIDAYS_FILE

from .closures import ClosureCalendar


def read_holidays(path=HOLIDAYS_FILE) -> list[dict]:
    """The holiday entries: {"date": "YYYY-MM-DD", "name": ..., "branches": [...]?}."""
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    return data["holidays"]


def applies_to(entry: dict, branch: str | None) -> bool:
    """National holidays close every branch; local ones only the listed branches."""
    branches = entry.get("branches")
    if not branches:
        return True
    return branch is not None and branch in branches


def holiday_dates(entries: list[dict], branch: str | None = None) -> set:
    """The days `branch` is closed for a holiday."""
    days = set()
    for entry in entries:
        if applies_to(entry, branch):
            days.add(date.fromisoformat(entry["date"]))
    return days


@lru_cache(maxsize=None)
def load_calendar(branch: str | None = None) -> ClosureCalendar:
    """The closure calendar of `branch` (national holidays only when None)."""
    return ClosureCalendar(holiday_dates(read_holidays(), branch))
