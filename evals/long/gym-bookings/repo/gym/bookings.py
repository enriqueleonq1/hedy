"""Reservas de clases."""
from .members import get_member
from .models import Booking, GymError
from .notifications import cancellation_text, confirmation_text, queue


def get_class(state, class_id):
    try:
        return state.classes[class_id]
    except KeyError:
        raise GymError(f"Clase inexistente: {class_id}") from None


def get_booking(state, booking_id):
    for booking in state.bookings:
        if booking.id == booking_id:
            return booking
    raise GymError(f"Reserva inexistente: {booking_id}")


def bookings_of_class(state, class_id):
    """Las reservas de una clase, en el orden en que se hicieron."""
    return [b for b in state.bookings if b.class_id == class_id]


def bookings_of_member(state, member_id):
    return [b for b in state.bookings if b.member_id == member_id]


def book(state, member_id, class_id, now):
    """Reserva una plaza y avisa al socio. Si la clase está llena, no se puede."""
    member = get_member(state, member_id)
    gym_class = get_class(state, class_id)
    if len(bookings_of_class(state, class_id)) >= gym_class.capacity:
        raise GymError("Clase llena")
    booking = Booking(
        id=state.next_booking, member_id=member.id, class_id=gym_class.id, created_at=now
    )
    state.next_booking += 1
    state.bookings.append(booking)
    queue(state, member.id, "Reserva", confirmation_text(gym_class), now)
    return booking


def cancel(state, booking_id, now):
    """Cancela una reserva y avisa al socio."""
    booking = get_booking(state, booking_id)
    gym_class = get_class(state, booking.class_id)
    state.bookings.remove(booking)
    queue(state, booking.member_id, "Reserva", cancellation_text(gym_class), now)
    return booking


def attendees(state, class_id):
    """Los nombres de quienes reservaron, en orden de reserva."""
    get_class(state, class_id)
    return [state.members[b.member_id].name for b in bookings_of_class(state, class_id)]


def free_places(state, class_id):
    gym_class = get_class(state, class_id)
    return gym_class.capacity - len(bookings_of_class(state, class_id))
