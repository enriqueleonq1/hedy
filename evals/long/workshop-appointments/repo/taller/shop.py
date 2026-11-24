"""El taller entero: carga los datos, los reúne y los guarda.

Es la entrada para el mostrador y la línea de comandos: cada operación que cambia
algo pasa por aquí.
"""
from datetime import date

from . import messages, quotes, storage
from .catalog import Catalog
from .customers import CustomerBook
from .errors import ShopError
from .inventory import Inventory
from .models import Message, PartUse, Quote, WorkOrder
from .orders import WorkOrders
from .staff import Staff


class Shop:
    def __init__(self, data: dict, path=None):
        self.path = path
        self.customers = CustomerBook(storage.customers_from(data), storage.vehicles_from(data))
        self.staff = Staff(storage.mechanics_from(data))
        self.catalog = Catalog(storage.services_from(data))
        self.inventory = Inventory(storage.parts_from(data))
        self.orders = WorkOrders(storage.orders_from(data))
        self.outbox: list[Message] = []

    @classmethod
    def load(cls, path=storage.DEFAULT_PATH) -> "Shop":
        return cls(storage.read(path), path)

    def to_dict(self) -> dict:
        return {
            "customers": storage.customers_to(self.customers.all()),
            "vehicles": storage.vehicles_to(self.customers.vehicles()),
            "mechanics": storage.mechanics_to(self.staff.all()),
            "services": storage.services_to(self.catalog.all()),
            "parts": storage.parts_to(self.inventory.all()),
            "orders": storage.orders_to(self.orders.all()),
        }

    def save(self, path=None) -> None:
        target = path or self.path
        if target is None:
            raise ShopError("No sé dónde guardar: falta el archivo")
        storage.write(self.to_dict(), target)

    # Órdenes de trabajo

    def open_order(self, plate: str, service_codes: list[str], mechanic_id: str | None = None,
                   day: str | None = None) -> WorkOrder:
        vehicle = self.customers.vehicle(plate)
        services = [self.catalog.get(code) for code in service_codes]
        if mechanic_id is not None:
            mechanic = self.staff.get(mechanic_id)
            for s in services:
                if not mechanic.can(s.skill):
                    raise ShopError(f"{mechanic.name} no hace {s.name}")
        return self.orders.create(vehicle.plate, mechanic_id, [s.code for s in services],
                                  day or date.today().isoformat())

    def add_part(self, order_id: int, code: str, qty: int) -> WorkOrder:
        self.inventory.get(code)
        return self.orders.add_part(order_id, code.strip().upper(), qty)

    def order_parts(self, order: WorkOrder) -> list[PartUse]:
        """Los repuestos de la orden: los de sus servicios y los agregados a mano."""
        total: dict[str, int] = {}
        for code in order.services:
            for use in self.catalog.get(code).parts:
                total[use.code] = total.get(use.code, 0) + use.qty
        for use in order.parts:
            total[use.code] = total.get(use.code, 0) + use.qty
        return [PartUse(code, qty) for code, qty in total.items()]

    def order_quote(self, order: WorkOrder) -> Quote:
        services = [self.catalog.get(code) for code in order.services]
        lines = quotes.lines_for(services, self.inventory, order.parts)
        return quotes.build(order.plate, lines, f"Orden #{order.id} — {order.plate}")

    def close_order(self, order_id: int, day: str | None = None) -> WorkOrder:
        order = self.orders.get(order_id)
        if not order.is_open:
            raise ShopError(f"La orden {order.id} ya está cerrada")
        uses = self.order_parts(order)
        for use in uses:
            part = self.inventory.get(use.code)
            if part.stock < use.qty:
                raise ShopError(f"No hay stock suficiente de {part.name}: quedan {part.stock}")
        for use in uses:
            self.inventory.take(use.code, use.qty)
        order.total_cents = self.order_quote(order).total_cents
        order.status = "cerrada"
        order.closed = day or date.today().isoformat()
        vehicle = self.customers.vehicle(order.plate)
        customer = self.customers.get(vehicle.customer)
        self.outbox.append(messages.order_ready(customer, vehicle, order))
        return order

    # Presupuestos

    def quote(self, plate: str, service_codes: list[str]) -> Quote:
        vehicle = self.customers.vehicle(plate)
        services = [self.catalog.get(code) for code in service_codes]
        return quotes.build(vehicle.plate, quotes.lines_for(services, self.inventory))

    def send_quote(self, plate: str, service_codes: list[str]) -> Quote:
        quote = self.quote(plate, service_codes)
        customer = self.customers.owner(plate)
        self.outbox.append(messages.quote_sent(customer, quote.plate, quote.total_cents))
        return quote
