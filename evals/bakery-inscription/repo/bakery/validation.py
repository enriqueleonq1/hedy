"""Checks an order before the counter accepts it."""
from datetime import date

from .catalog import PRODUCTS
from .hours import is_open
from .models import Order

# Longest text the label printer fits, by Order field.
MAX_TEXT = {
    "note": 200,
    "delivery_address": 120,
}


def problems(order: Order, received_on: date | None = None) -> list[str]:
    """What is wrong with the order, in Spanish; empty when it is fine."""
    found = []
    for line in order.lines:
        if line.product not in PRODUCTS:
            found.append(f"no vendemos «{line.product}»")
        if line.qty <= 0:
            found.append(f"cantidad inválida para «{line.product}»")
    if not is_open(order.pickup_date, order.pickup_time):
        found.append("la panadería está cerrada a esa hora")
    for name, limit in MAX_TEXT.items():
        value = getattr(order, name)
        if value is not None and len(value) > limit:
            found.append(f"{name} supera {limit} caracteres")
    if received_on is not None:
        for line in order.lines:
            product = PRODUCTS.get(line.product)
            if product and (order.pickup_date - received_on).days < product.needs_notice_days:
                found.append(f"«{product.name}» se encarga con {product.needs_notice_days} día de anticipación")
    return found
