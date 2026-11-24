"""Los mensajes que el taller manda a sus clientes."""
from . import money
from .models import Customer, Message, Vehicle, WorkOrder


def contact(customer: Customer) -> str:
    """A dónde se le escribe: el teléfono si hay, si no el correo."""
    return customer.phone or customer.email


def order_ready(customer: Customer, vehicle: Vehicle, order: WorkOrder) -> Message:
    text = (
        f"Hola {customer.name.split()[0]}, tu {vehicle.brand} {vehicle.model} ({vehicle.plate}) ya está listo. "
        f"Total: {money.fmt(order.total_cents)}. Te esperamos de 08:00 a 18:00."
    )
    return Message(contact(customer), text)


def quote_sent(customer: Customer, plate: str, total_cents: int) -> Message:
    return Message(contact(customer), f"Te enviamos el presupuesto para {plate}: {money.fmt(total_cents)} con IVA.")


NOTICE_HOURS = 24


def reminder_text(customer: Customer, appointment, service_name: str) -> str:
    """El recordatorio del turno, tal como lo recibe el cliente."""
    day = appointment.day.strftime("%d/%m")
    return (
        f"Hola {customer.name.split()[0]}, te esperamos el {day} a las {appointment.start} "
        f"para {service_name} (patente {appointment.plate}). "
        f"Si no puedes venir, avísanos con {NOTICE_HOURS} horas de anticipación."
    )


def reminder(customer: Customer, appointment, service_name: str) -> Message:
    return Message(customer.phone, reminder_text(customer, appointment, service_name))
