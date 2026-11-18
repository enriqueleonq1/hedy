from dataclasses import dataclass


@dataclass
class Product:
    name: str
    price_cents: int


CATALOG = [
    Product("Corte de cabello", 1250),
    Product("Manicure", 900),
]
