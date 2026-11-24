"""What an order costs: its products plus the extras it asked for."""
from .catalog import EXTRAS_CENTS, product
from .models import Order

# (label on the ticket, key in EXTRAS_CENTS, whether the order has it)
EXTRA_RULES = [
    ("Envío a domicilio", "delivery", lambda o: o.delivery_address is not None),
    ("Caja de regalo", "gift_box", lambda o: o.gift_box),
    ("Dedicatoria", "inscription", lambda o: o.inscription is not None),
]


def price_lines(order: Order) -> list[tuple[str, int]]:
    """(label, cents) for each product line, then each extra that applies."""
    lines = []
    for line in order.lines:
        p = product(line.product)
        label = f"{line.qty} x {p.name}"
        lines.append((label, line.qty * p.price_cents))
    for label, key, applies in EXTRA_RULES:
        if applies(order):
            lines.append((label, EXTRAS_CENTS[key]))
    return lines


def order_total_cents(order: Order) -> int:
    return sum(cents for _, cents in price_lines(order))


def day_total_cents(orders: list[Order]) -> int:
    return sum(order_total_cents(o) for o in orders)
