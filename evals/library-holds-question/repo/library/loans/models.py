from dataclasses import dataclass
from datetime import date


@dataclass
class Loan:
    id: str
    item_id: str
    member_id: str
    branch: str
    checked_out: date
    returned: date | None = None
    returned_at: str | None = None
    renewals: int = 0

    @property
    def active(self) -> bool:
        return self.returned is None
