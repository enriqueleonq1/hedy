"""Correos a los socios. No se envían desde aquí: quedan en la bandeja de
salida (store.outbox) y otro proceso los manda."""

from .dates import month_name
from .money import eur


def queue(store, to, subject, body):
    message = {"to": to, "subject": subject, "body": body}
    store.outbox.append(message)
    return message


def welcome_email(member):
    return (
        f"Hola {member.first_name}, ¡bienvenido al coworking! "
        "Tu tarjeta de acceso ya está activa."
    )


def booking_email(store, booking):
    room = store.rooms[booking.room]
    return (
        f"Reserva confirmada: sala {room.name} el {booking.day.strftime('%d/%m/%Y')} "
        f"de {booking.start} a {booking.end}."
    )


def invoice_email(store, number):
    """El correo de una factura: 'Hola Ana, tu factura F-2025-0001 de marzo
    2025 por 289,67 € ya está disponible.'"""
    from .billing import get_invoice
    from .members import find_member

    item = get_invoice(store, number)
    member = find_member(store, item.member_id)
    return (
        f"Hola {member.first_name}, tu factura {item.number} de {month_name(item.month)} "
        f"por {eur(item.total_cents)} ya está disponible."
    )


def send_welcome(store, member):
    return queue(store, member.email, "Bienvenida", welcome_email(member))


def send_booking(store, booking):
    from .members import find_member

    member = find_member(store, booking.member)
    return queue(store, member.email, "Reserva de sala", booking_email(store, booking))


def send_invoice(store, item):
    from .members import find_member

    member = find_member(store, item.member_id)
    return queue(store, member.email, f"Factura {item.number}", invoice_email(store, item.number))
