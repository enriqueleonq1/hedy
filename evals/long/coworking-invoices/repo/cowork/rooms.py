"""Salas de reuniones y sus reservas."""

from .dates import minutes, month_key
from .models import Booking
from .validation import check_time


def find_room(store, room_id):
    try:
        return store.rooms[room_id]
    except KeyError:
        raise ValueError(f"Sala desconocida: {room_id}") from None


def book(store, member_id, room_id, day, start, end):
    """Reserva una sala para un socio. Devuelve la reserva con su id."""
    from .members import find_member

    find_member(store, member_id)
    find_room(store, room_id)
    check_time(start)
    check_time(end)
    if minutes(start) >= minutes(end):
        raise ValueError("La reserva tiene que terminar después de empezar")
    booking = Booking(store.next_booking, member_id, room_id, day, start, end)
    store.next_booking += 1
    store.bookings.append(booking)
    return booking


def find_booking(store, booking_id):
    for booking in store.bookings:
        if booking.id == booking_id:
            return booking
    return None


def bookings_on(store, day, room_id=None):
    found = [b for b in store.bookings if b.day == day and (room_id is None or b.room == room_id)]
    return sorted(found, key=lambda b: (b.room, minutes(b.start)))


def duration(booking):
    return minutes(booking.end) - minutes(booking.start)


def member_minutes(store, member_id, month):
    """Minutos de sala que reservó un socio en el mes."""
    return sum(
        duration(b) for b in store.bookings if b.member == member_id and month_key(b.day) == month
    )


def schedule_text(store, day):
    """La agenda de salas de un día, sala por sala."""
    lines = [f"Salas el {day.isoformat()}"]
    for room in store.rooms.values():
        lines.append(f"{room.name}:")
        todays = bookings_on(store, day, room.id)
        if not todays:
            lines.append("  (libre)")
        for booking in todays:
            lines.append(f"  {booking.start}-{booking.end} {booking.member}")
    return "\n".join(lines)
