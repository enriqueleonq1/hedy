"""Repuestos y su stock."""
from .errors import NotFound, ShopError, StockError
from .models import Part


class Inventory:
    """El stock de cada repuesto y lo reservado para turnos que todavía no se cerraron.

    Lo disponible es el stock menos lo reservado: es lo que se puede prometer a un turno nuevo.
    """

    def __init__(self, parts: list[Part]):
        self._parts = {p.code: p for p in parts}
        self._reserved: dict[str, int] = {}

    def all(self) -> list[Part]:
        return list(self._parts.values())

    def get(self, code: str) -> Part:
        try:
            return self._parts[code.strip().upper()]
        except KeyError:
            raise NotFound(f"Repuesto inexistente: {code}") from None

    def stock(self, code: str) -> int:
        return self.get(code).stock

    def reserved(self, code: str) -> int:
        return self._reserved.get(self.get(code).code, 0)

    def available(self, code: str) -> int:
        """El stock que no está reservado para un turno."""
        return self.stock(code) - self.reserved(code)

    def missing(self, uses) -> Part | None:
        """El primer repuesto de `uses` cuyo disponible no alcanza, si hay alguno."""
        for use in uses:
            if self.available(use.code) < use.qty:
                return self.get(use.code)
        return None

    def reserve(self, uses) -> None:
        """Reserva los repuestos de un turno; todos o ninguno."""
        part = self.missing(uses)
        if part is not None:
            raise StockError(f"Sin stock de {part.name} para ese servicio")
        for use in uses:
            code = self.get(use.code).code
            self._reserved[code] = self._reserved.get(code, 0) + use.qty

    def release(self, uses) -> None:
        """Devuelve lo reservado para un turno (cancelado o cerrado)."""
        for use in uses:
            code = self.get(use.code).code
            left = self._reserved.get(code, 0) - use.qty
            if left > 0:
                self._reserved[code] = left
            else:
                self._reserved.pop(code, None)

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
        """Los repuestos cuyo disponible (stock menos lo reservado) está en el mínimo o por debajo."""
        return [p for p in self._parts.values() if self.available(p.code) <= p.min_stock]

    def value_cents(self) -> int:
        return sum(p.stock * p.price_cents for p in self._parts.values())
