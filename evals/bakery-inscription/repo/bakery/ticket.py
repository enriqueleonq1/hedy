"""The ticket printed for the customer and stuck on the box."""
from .money import fmt
from .models import Order
from .pricing import order_total_cents, price_lines

WIDTH = 32

# (label, how to read it from the order); only printed when the order has it.
DETAILS = [
    ("Nota", lambda o: o.note),
    ("Domicilio", lambda o: o.delivery_address),
]


def _row(left: str, right: str) -> str:
    gap = max(WIDTH - len(left) - len(right), 1)
    return f"{left}{' ' * gap}{right}"


def render_ticket(order: Order) -> str:
    out = [
        "PANADERÍA LA ESPIGA".center(WIDTH),
        "-" * WIDTH,
        f"Cliente: {order.customer}",
        f"Retira: {order.pickup_date.strftime('%d/%m/%Y')} {order.pickup_time.strftime('%H:%M')}",
    ]
    for label, read in DETAILS:
        value = read(order)
        if value:
            out.append(f"{label}: {value}")
    out.append("-" * WIDTH)
    for label, cents in price_lines(order):
        out.append(_row(label, fmt(cents)))
    out.append("-" * WIDTH)
    out.append(_row("TOTAL", fmt(order_total_cents(order))))
    out.append("PAGADO" if order.paid else "A PAGAR AL RETIRAR")
    return "\n".join(out)
