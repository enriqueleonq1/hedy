"""Order in which waiting holds are served when a copy comes back."""
from datetime import date

from library.policy.rules import LOCAL_PREFERENCE_DAYS

from .models import Hold


def waited_days(hold: Hold, on: date) -> int:
    return (on - hold.placed_on).days


def hold_sort_key(hold: Hold, returned_at: str, on: date) -> tuple:
    """Sort key for the holds competing for a copy returned at `returned_at` on `on`.

    Sending a copy to another branch takes days, so holds picked up where the
    copy is already go first. A hold that has waited more than
    LOCAL_PREFERENCE_DAYS no longer gives way to local ones. Ties go to the
    oldest hold.
    """
    long_wait = waited_days(hold, on) > LOCAL_PREFERENCE_DAYS
    local = hold.pickup_branch == returned_at
    return (0 if long_wait else 1, 0 if long_wait or local else 1, hold.placed_on, hold.id)
