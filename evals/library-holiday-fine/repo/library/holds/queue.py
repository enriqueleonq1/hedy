"""The holds waiting for a title."""
from datetime import date

from library.policy.rules import MAX_HOLDS_PER_MEMBER

from .models import Hold


class HoldError(Exception):
    pass


def waiting_holds(holds: list[Hold], title_id: str) -> list[Hold]:
    """Holds still waiting for a copy of `title_id`, in the order they were placed."""
    waiting = [h for h in holds if h.title_id == title_id and h.status == "waiting"]
    return sorted(waiting, key=lambda h: (h.placed_on, h.id))


def position(holds: list[Hold], hold_id: str) -> int:
    """1-based place of a hold among the holds of its title (by placement date)."""
    hold = next(h for h in holds if h.id == hold_id)
    for index, other in enumerate(waiting_holds(holds, hold.title_id), start=1):
        if other.id == hold_id:
            return index
    raise HoldError(f"hold {hold_id} is not waiting")


def place_hold(holds: list[Hold], hold_id: str, member_id: str, title_id: str, branch: str, on: date) -> Hold:
    mine = [h for h in holds if h.member_id == member_id and h.status in ("waiting", "ready")]
    if any(h.title_id == title_id for h in mine):
        raise HoldError("already_holding")
    if len(mine) >= MAX_HOLDS_PER_MEMBER:
        raise HoldError("too_many_holds")
    hold = Hold(hold_id, member_id, title_id, branch, on)
    holds.append(hold)
    return hold


def cancel(holds: list[Hold], hold_id: str) -> None:
    for hold in holds:
        if hold.id == hold_id and hold.status in ("waiting", "ready"):
            hold.status = "cancelled"
            return
    raise HoldError(f"no active hold {hold_id}")


def expire_uncollected(holds: list[Hold], on: date) -> list[Hold]:
    """Ready holds whose pickup day passed become expired; returns them."""
    expired = []
    for hold in holds:
        if hold.status == "ready" and hold.pickup_by is not None and on > hold.pickup_by:
            hold.status = "expired"
            expired.append(hold)
    return expired
