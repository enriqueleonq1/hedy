from dataclasses import dataclass


@dataclass
class Product:
    sku: str
    name: str
    stock: int


@dataclass
class Reservation:
    id: int
    sku: str
    qty: int
    created_at: float
    confirmed: bool = False
