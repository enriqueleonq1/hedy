"""Orders live as .pedido files in a folder per day."""
from pathlib import Path

from .models import Order
from .orderfile import parse_order

SUFFIX = ".pedido"


def load_order(path: Path) -> Order:
    return parse_order(Path(path).read_text(encoding="utf-8"), source=Path(path).name)


def load_orders(folder: Path) -> list[Order]:
    """Every order in `folder`, sorted by pickup time and then file name."""
    orders = [load_order(p) for p in sorted(Path(folder).glob(f"*{SUFFIX}"))]
    orders.sort(key=lambda o: (o.pickup_date, o.pickup_time, o.source))
    return orders
