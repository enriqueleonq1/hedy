"""Correos a los socios. No se envían desde aquí: quedan en la bandeja de
salida (store.outbox) y otro proceso los manda."""


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


def send_welcome(store, member):
    return queue(store, member.email, "Bienvenida", welcome_email(member))


def send_booking(store, booking):
    from .members import find_member

    member = find_member(store, booking.member)
    return queue(store, member.email, "Reserva de sala", booking_email(store, booking))
