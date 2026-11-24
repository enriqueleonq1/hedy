"""El taller entero: carga los datos, los reúne y los guarda.

Es la entrada para el mostrador y la línea de comandos: cada operación que cambia
algo pasa por aquí.
"""
from datetime import date

from . import export, messages, quotes, reports, storage
from .agenda import Agenda, check_hours
from .catalog import Catalog
from .customers import CustomerBook
from .errors import BookingError, NotFound, ShopError, StockError
from .inventory import Inventory
from .models import Appointment, Message, PartUse, Quote, WorkOrder
from .validation import normalize_plate
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
        self.agenda = Agenda(storage.appointments_from(data))
        self.outbox: list[Message] = []
        # Lo que esperan los turnos que siguen en pie sale de lo disponible.
        for a in self.agenda.all():
            if a.status in ("reservado", "en_taller"):
                self.inventory.reserve(self.catalog.get(a.service).parts)

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
            "appointments": storage.appointments_to(self.agenda.all()),
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

    def is_frequent(self, plate: str, before_order: int | None = None) -> bool:
        """El dueño del vehículo tiene 3 órdenes cerradas o más (antes de `before_order`, si se da)."""
        customer = self.customers.owner(plate)
        plates = {v.plate for v in self.customers.vehicles_of(customer.id)}
        closed = [
            o for o in self.orders.all()
            if o.plate in plates and o.status == "cerrada" and (before_order is None or o.id < before_order)
        ]
        return len(closed) >= quotes.FREQUENT_ORDERS

    def _lines(self, plate: str, services, extra_parts=(), before_order: int | None = None):
        lines = quotes.lines_for(services, self.inventory, extra_parts)
        if self.is_frequent(plate, before_order):
            lines.append(quotes.frequent_discount(lines))
        return lines

    def order_quote(self, order: WorkOrder) -> Quote:
        services = [self.catalog.get(code) for code in order.services]
        lines = self._lines(order.plate, services, order.parts, before_order=order.id)
        return quotes.build(order.plate, lines, f"Orden #{order.id} — {order.plate}")

    def close_order(self, order_id: int, day: str | None = None) -> WorkOrder:
        order = self.orders.get(order_id)
        if not order.is_open:
            raise ShopError(f"La orden {order.id} ya está cerrada")
        appointment = self._appointment_of(order)
        uses = self.order_parts(order)
        for use in uses:
            part = self.inventory.get(use.code)
            if part.stock < use.qty:
                raise ShopError(f"No hay stock suficiente de {part.name}: quedan {part.stock}")
        if appointment is not None:
            # Lo reservado para el turno ahora sale del stock.
            self.inventory.release(self.catalog.get(appointment.service).parts)
            appointment.status = "terminado"
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
        return quotes.build(vehicle.plate, self._lines(vehicle.plate, services))

    def send_quote(self, plate: str, service_codes: list[str]) -> Quote:
        quote = self.quote(plate, service_codes)
        customer = self.customers.owner(plate)
        self.outbox.append(messages.quote_sent(customer, quote.plate, quote.total_cents))
        return quote

    # Turnos

    def _service_for_booking(self, code: str):
        if not self.catalog.exists(code):
            raise BookingError(f"Servicio desconocido: {code}")
        service = self.catalog.get(code)
        mechanics = self.staff.with_skill(service.skill)
        if not mechanics:
            raise BookingError("Ningún mecánico hace ese servicio")
        return service, mechanics

    def book(self, plate: str, service_code: str, day: date, start: str) -> Appointment:
        """Da un turno, reserva sus repuestos y le manda el recordatorio al cliente."""
        plate = normalize_plate(plate)
        try:
            vehicle = self.customers.vehicle(plate)
        except NotFound:
            raise BookingError(f"Vehículo no registrado: {plate}") from None
        customer = self.customers.get(vehicle.customer)
        if not customer.phone:
            raise BookingError("El cliente no tiene teléfono para avisarle")
        service, mechanics = self._service_for_booking(service_code)
        end = check_hours(day, start, service.minutes)
        if self.agenda.busy(day, start, end, plate=plate):
            raise BookingError("Ese auto ya tiene un turno en ese horario")
        mechanic = self.agenda.free_mechanic(mechanics, day, start, end)
        if mechanic is None:
            raise BookingError("No hay mecánicos libres en ese horario")
        try:
            self.inventory.reserve(service.parts)
        except StockError as error:
            raise BookingError(str(error)) from None
        appointment = self.agenda.add(
            Appointment(self.agenda.next_id(), plate, service.code, mechanic.id, day, start, end)
        )
        self.outbox.append(messages.reminder(customer, appointment, service.name))
        return appointment

    def appointment(self, appointment_id: int) -> Appointment:
        return self.agenda.get(appointment_id)

    def cancel(self, appointment_id: int) -> Appointment:
        """Cancela un turno: libera la franja del mecánico y devuelve sus repuestos."""
        appointment = self.agenda.active(appointment_id)
        if appointment.status == "en_taller":
            raise BookingError("El turno ya está en el taller")
        if appointment.status == "terminado":
            raise BookingError("El turno ya terminó")
        appointment.status = "cancelado"
        self.inventory.release(self.catalog.get(appointment.service).parts)
        return appointment

    def free_slots(self, day: date, service_code: str) -> list[str]:
        service, mechanics = self._service_for_booking(service_code)
        return self.agenda.slots(mechanics, day, service.minutes)

    def reminder(self, appointment_id: int) -> str:
        appointment = self.agenda.active(appointment_id)
        customer = self.customers.owner(appointment.plate)
        return messages.reminder_text(customer, appointment, self.catalog.get(appointment.service).name)

    def day_text(self, day: date) -> str:
        return reports.day_text(self.staff, self.catalog, self.agenda.on(day), day)

    def start_order(self, appointment_id: int) -> WorkOrder:
        """El auto llegó: abre su orden de trabajo con el servicio y el mecánico del turno."""
        appointment = self.agenda.active(appointment_id)
        if appointment.status == "en_taller":
            raise BookingError("El turno ya está en el taller")
        if appointment.status == "terminado":
            raise BookingError("El turno ya terminó")
        order = self.orders.create(appointment.plate, appointment.mechanic, [appointment.service],
                                   appointment.day.isoformat())
        appointment.status = "en_taller"
        appointment.order = order.id
        return order

    def _appointment_of(self, order: WorkOrder) -> Appointment | None:
        for a in self.agenda.all():
            if a.order == order.id:
                return a
        return None

    def appointment_quote(self, appointment_id: int) -> Quote:
        appointment = self.agenda.active(appointment_id)
        services = [self.catalog.get(appointment.service)]
        lines = self._lines(appointment.plate, services)
        return quotes.build(appointment.plate, lines, f"Presupuesto turno #{appointment.id} — {appointment.plate}")

    def appointments_csv(self, day: date) -> str:
        return export.appointments_csv(self, day)
