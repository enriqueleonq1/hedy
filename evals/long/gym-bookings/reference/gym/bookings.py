"""Reservas de clases: plazas, lista de espera, cancelaciones, check-in e
inasistencias."""
from datetime import timedelta

from .members import freeze_on, get_member, plan_on
from .models import Booking, GymError
from .notifications import (
    cancellation_text,
    confirmation_text,
    promoted_text,
    queue,
    reminder_text,
    waitlist_text,
)
from .pricing import ACTIVE_LIMIT

# Hasta cuánto antes del inicio se puede cancelar.
CANCEL_BEFORE = timedelta(hours=2)
# El check-in abre esto antes del inicio y cierra al terminar la clase.
CHECK_IN_OPENS = timedelta(minutes=30)
# Dos inasistencias en esta ventana bloquean las reservas...
NO_SHOW_WINDOW = timedelta(days=30)
NO_SHOWS_TO_BLOCK = 2
# ...hasta esto después del día de la última.
BLOCK_FOR = timedelta(days=7)


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
    """Las reservas con plaza de una clase, en el orden en que se hicieron."""
    return [b for b in state.bookings if b.class_id == class_id and b.status == "plaza"]


def waiting_of_class(state, class_id):
    """La lista de espera de una clase, en orden."""
    return [b for b in state.bookings if b.class_id == class_id and b.status == "espera"]


def bookings_of_member(state, member_id):
    return [b for b in state.bookings if b.member_id == member_id]


def active_places(state, member_id, now):
    """Reservas con plaza del socio en clases que todavía no empezaron."""
    return [
        b
        for b in bookings_of_member(state, member_id)
        if b.status == "plaza" and state.classes[b.class_id].start > now
    ]


def no_show_bookings(state, member_id, now):
    """Reservas con plaza del socio en clases ya terminadas sin check-in."""
    return [
        b
        for b in bookings_of_member(state, member_id)
        if b.status == "plaza" and not b.checked_in and state.classes[b.class_id].end <= now
    ]


def blocked_until(state, member_id, now):
    """El día desde el que el socio puede volver a reservar, si está bloqueado."""
    recent = [
        state.classes[b.class_id]
        for b in no_show_bookings(state, member_id, now)
        if now - state.classes[b.class_id].end <= NO_SHOW_WINDOW
    ]
    if len(recent) < NO_SHOWS_TO_BLOCK:
        return None
    last = max(c.start for c in recent)
    until = last.date() + BLOCK_FOR
    return until if now.date() < until else None


def book(state, member_id, class_id, now):
    """Reserva una plaza, o un lugar en la lista de espera si la clase está
    llena, y avisa al socio."""
    member = get_member(state, member_id)
    gym_class = get_class(state, class_id)
    if now >= gym_class.start:
        raise GymError("La clase ya empezó")
    if any(b.class_id == gym_class.id for b in bookings_of_member(state, member.id)):
        raise GymError("Ya tienes una reserva para esta clase")
    period = freeze_on(state, member.id, gym_class.day)
    if period:
        raise GymError(f"Tu membresía está congelada hasta el {period.end:%d/%m}")
    until = blocked_until(state, member.id, now)
    if until:
        raise GymError(f"Reservas bloqueadas hasta el {until:%d/%m} por inasistencias")
    full = len(bookings_of_class(state, gym_class.id)) >= gym_class.capacity
    if not full:
        plan = plan_on(state, member.id, now.date())
        limit = ACTIVE_LIMIT.get(plan)
        if limit is not None and len(active_places(state, member.id, now)) >= limit:
            raise GymError(f"Tu plan {plan} permite {limit} reservas activas")
    booking = Booking(
        id=state.next_booking,
        member_id=member.id,
        class_id=gym_class.id,
        created_at=now,
        status="espera" if full else "plaza",
    )
    state.next_booking += 1
    state.bookings.append(booking)
    if full:
        position = len(waiting_of_class(state, gym_class.id))
        queue(state, member.id, "Lista de espera", waitlist_text(position), now)
    else:
        queue(state, member.id, "Reserva", confirmation_text(gym_class), now)
    return booking


def cancel(state, booking_id, now):
    """Cancela una reserva y avisa al socio. Si tenía plaza, la toma el
    primero de la lista de espera."""
    booking = get_booking(state, booking_id)
    gym_class = get_class(state, booking.class_id)
    if gym_class.start - now < CANCEL_BEFORE:
        hours = int(CANCEL_BEFORE.total_seconds() // 3600)
        raise GymError(f"Ya no se puede cancelar: faltan menos de {hours} horas")
    state.bookings.remove(booking)
    queue(state, booking.member_id, "Reserva", cancellation_text(gym_class), now)
    if booking.status == "plaza":
        waiting = waiting_of_class(state, gym_class.id)
        if waiting:
            promoted = waiting[0]
            promoted.status = "plaza"
            queue(state, promoted.member_id, "Reserva", promoted_text(gym_class), now)
    return booking


def attendees(state, class_id):
    """Los nombres de quienes tienen plaza, en orden de reserva."""
    get_class(state, class_id)
    return [state.members[b.member_id].name for b in bookings_of_class(state, class_id)]


def waitlist(state, class_id):
    """Los nombres de la lista de espera, en orden."""
    get_class(state, class_id)
    return [state.members[b.member_id].name for b in waiting_of_class(state, class_id)]


def free_places(state, class_id):
    gym_class = get_class(state, class_id)
    return gym_class.capacity - len(bookings_of_class(state, class_id))


def check_in(state, booking_id, now):
    """Registra que el socio llegó a la clase."""
    booking = get_booking(state, booking_id)
    gym_class = get_class(state, booking.class_id)
    if booking.status != "plaza":
        raise GymError("Solo puede hacer check-in quien tiene plaza")
    if not gym_class.start - CHECK_IN_OPENS <= now <= gym_class.end:
        raise GymError("Check-in fuera de horario")
    booking.checked_in = True
    return booking


def no_shows(state, class_id, now):
    """Quienes tenían plaza y no hicieron check-in, una vez terminada la clase."""
    gym_class = get_class(state, class_id)
    if now < gym_class.end:
        return []
    return [
        state.members[b.member_id].name
        for b in bookings_of_class(state, class_id)
        if not b.checked_in
    ]


def send_reminders(state, now):
    """Un recordatorio a cada socio con plaza en una clase de mañana, una sola
    vez por reserva. Devuelve cuántos se mandaron."""
    tomorrow = now.date() + timedelta(days=1)
    sent = 0
    for booking in list(state.bookings):
        gym_class = state.classes[booking.class_id]
        if booking.status != "plaza" or gym_class.day != tomorrow:
            continue
        if booking.id in state.reminded:
            continue
        queue(state, booking.member_id, "Recordatorio", reminder_text(gym_class), now)
        state.reminded.add(booking.id)
        sent += 1
    return sent
