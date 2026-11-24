"""Órdenes de trabajo: lo que se le hace a un vehículo y lo que cuesta."""
from .errors import NotFound, ShopError
from .models import PartUse, WorkOrder


class WorkOrders:
    def __init__(self, orders: list[WorkOrder]):
        self._orders = {o.id: o for o in orders}

    def all(self) -> list[WorkOrder]:
        return sorted(self._orders.values(), key=lambda o: o.id)

    def open(self) -> list[WorkOrder]:
        return [o for o in self.all() if o.is_open]

    def get(self, order_id: int) -> WorkOrder:
        try:
            return self._orders[int(order_id)]
        except (KeyError, ValueError):
            raise NotFound(f"Orden inexistente: {order_id}") from None

    def next_id(self) -> int:
        return max(self._orders, default=0) + 1

    def create(self, plate: str, mechanic: str | None, services: list[str], day: str) -> WorkOrder:
        if not services:
            raise ShopError("Una orden necesita al menos un servicio")
        order = WorkOrder(self.next_id(), plate, mechanic, list(services), [], day)
        self._orders[order.id] = order
        return order

    def add_part(self, order_id: int, code: str, qty: int) -> WorkOrder:
        order = self.get(order_id)
        if not order.is_open:
            raise ShopError(f"La orden {order.id} ya está cerrada")
        if qty <= 0:
            raise ShopError("La cantidad tiene que ser positiva")
        for use in order.parts:
            if use.code == code:
                use.qty += qty
                return order
        order.parts.append(PartUse(code, qty))
        return order

    def of_plate(self, plate: str) -> list[WorkOrder]:
        return [o for o in self.all() if o.plate == plate]

    def closed_in(self, month: str) -> list[WorkOrder]:
        """Las órdenes cerradas en el mes 'AAAA-MM'."""
        return [o for o in self.all() if o.status == "cerrada" and (o.closed or "").startswith(month)]
