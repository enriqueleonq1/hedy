from dataclasses import dataclass
from datetime import date

# waiting -> ready (copy set aside) -> collected; waiting/ready -> cancelled/expired
STATUSES = ("waiting", "ready", "collected", "cancelled", "expired")


@dataclass
class Hold:
    """A member waiting for any copy of a title."""

    id: str
    member_id: str
    title_id: str
    pickup_branch: str
    placed_on: date
    status: str = "waiting"
    item_id: str | None = None
    pickup_by: date | None = None

    def __post_init__(self):
        if self.status not in STATUSES:
            raise ValueError(f"unknown hold status: {self.status}")
