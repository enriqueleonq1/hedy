"""What we sell and what it costs. Prices are integer cents."""
from dataclasses import dataclass


@dataclass(frozen=True)
class Product:
    name: str
    price_cents: int
    needs_notice_days: int = 0


PRODUCTS = {
    "baguette": Product("Baguette", 120),
    "pan de campo": Product("Pan de campo", 380),
    "medialuna": Product("Medialuna", 90),
    "croissant": Product("Croissant", 150),
    "alfajor": Product("Alfajor de maicena", 110),
    "pastafrola": Product("Pastafrola", 950),
    "tarta de manzana": Product("Tarta de manzana", 1400),
    "torta de chocolate": Product("Torta de chocolate", 2600, needs_notice_days=1),
    "torta de frutillas": Product("Torta de frutillas", 2900, needs_notice_days=1),
    "lemon pie": Product("Lemon pie", 1800),
}

# Charges added once per order, by extra.
EXTRAS_CENTS = {
    "delivery": 350,
    "gift_box": 150,
    "inscription": 200,
}


def product(key: str) -> Product:
    return PRODUCTS[key.strip().lower()]
