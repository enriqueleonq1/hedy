from dataclasses import dataclass, field
from datetime import date, time


@dataclass(frozen=True)
class Line:
    qty: int
    product: str


@dataclass
class Order:
    """One order, as written in a .pedido file.

    Optional fields stay None/False when the file does not mention them.
    """

    customer: str
    pickup_date: date
    pickup_time: time
    lines: list[Line] = field(default_factory=list)
    note: str | None = None
    delivery_address: str | None = None
    gift_box: bool = False
    paid: bool = False
    source: str = ""
