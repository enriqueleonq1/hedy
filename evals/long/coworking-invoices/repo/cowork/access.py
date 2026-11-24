"""Control de acceso: quién entra por qué puerta y cuándo."""

from .dates import month_key
from .members import find_member
from .models import AccessEntry
from .validation import check_time

DOORS = ("principal", "garaje")


def can_enter(member, day):
    if member.joined > day:
        return False
    return member.left is None or member.left >= day


def enter(store, member_id, day, time, door="principal"):
    """Registra una entrada. Solo pasan los socios en alta ese día."""
    member = find_member(store, member_id)
    check_time(time)
    if door not in DOORS:
        raise ValueError(f"Puerta desconocida: {door}")
    if not can_enter(member, day):
        raise ValueError(f"Acceso denegado: {member_id}")
    entry = AccessEntry(member_id, day, time, door)
    store.access_log.append(entry)
    return entry


def entries_on(store, day):
    return sorted((e for e in store.access_log if e.day == day), key=lambda e: e.time)


def visits_in_month(store, member_id, month):
    """Días distintos del mes en que el socio entró."""
    return len({e.day for e in store.access_log if e.member == member_id and month_key(e.day) == month})
