"""Repuestos y su stock."""
from .errors import NotFound, ShopError, StockError
from .models import Part


class Inventory:
    def __init__(self, parts: list[Part]):
        self._parts = {p.code: p for p in parts}

    def all(self) -> list[Part]:
        return list(self._parts.values())

    def get(self, code: str) -> Part:
        try:
            return self._parts[code.strip().upper()]
        except KeyError:
            raise NotFound(f"Repuesto inexistente: {code}") from None

    def stock(self, code: str) -> int:
        return self.get(code).stock

    def take(self, code: str, qty: int) -> Part:
        """Saca qty unidades del stock (al cerrar una orden)."""
        if qty <= 0:
            raise ShopError("La cantidad tiene que ser positiva")
        part = self.get(code)
        if part.stock < qty:
            raise StockError(f"No hay stock suficiente de {part.name}: quedan {part.stock}")
        part.stock -= qty
        return part

    def restock(self, code: str, qty: int) -> Part:
        if qty <= 0:
            raise ShopError("La cantidad tiene que ser positiva")
        part = self.get(code)
        part.stock += qty
        return part

    def low_stock(self) -> list[Part]:
        """Los repuestos con stock en el mínimo o por debajo."""
        return [p for p in self._parts.values() if p.stock <= p.min_stock]

    def value_cents(self) -> int:
        return sum(p.stock * p.price_cents for p in self._parts.values())
