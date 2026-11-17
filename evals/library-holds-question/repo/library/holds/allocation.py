"""Who gets a copy that was just returned."""
from datetime import date

from library.calendar.loader import load_calendar
from library.catalog.models import Item
from library.members.standing import may_receive_holds
from library.policy.rules import HOLD_PICKUP_DAYS
from library.state import Library

from .models import Hold
from .priority import hold_sort_key
from .queue import waiting_holds


def candidates(library: Library, title_id: str) -> list[Hold]:
    """Waiting holds of members who may receive a copy right now."""
    return [
        hold
        for hold in waiting_holds(library.holds, title_id)
        if may_receive_holds(library.members[hold.member_id], library.ledger)
    ]


def allocate_returned_copy(library: Library, item: Item, returned_at: str, on: date) -> Hold | None:
    """Sets the returned copy aside for the hold that goes first, if any.

    The chosen hold becomes "ready" and has HOLD_PICKUP_DAYS open days of its
    pickup branch to be collected.
    """
    competing = candidates(library, item.title_id)
    if not competing:
        return None
    competing.sort(key=lambda h: hold_sort_key(h, returned_at, on))
    chosen = competing[0]
    chosen.status = "ready"
    chosen.item_id = item.id
    chosen.pickup_by = load_calendar(chosen.pickup_branch).add_open_days(on, HOLD_PICKUP_DAYS)
    return chosen
