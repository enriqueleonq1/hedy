"""When the shop is open for pickups."""
from datetime import date, time

# weekday() -> (opens, closes); Monday is closed.
OPENING_HOURS = {
    1: (time(7, 30), time(20, 0)),
    2: (time(7, 30), time(20, 0)),
    3: (time(7, 30), time(20, 0)),
    4: (time(7, 30), time(20, 0)),
    5: (time(7, 30), time(21, 0)),
    6: (time(8, 0), time(13, 0)),
}


def is_open(day: date, at: time) -> bool:
    hours = OPENING_HOURS.get(day.weekday())
    return hours is not None and hours[0] <= at <= hours[1]
