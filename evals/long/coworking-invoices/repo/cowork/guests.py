"""Invitados: un socio puede traer invitados; como mucho 2 por día y socio."""

from .access import can_enter
from .dates import month_key
from .members import find_member
from .validation import check_name

PER_DAY = 2


def invite(store, member_id, guest_name, day):
    member = find_member(store, member_id)
    if not can_enter(member, day):
        raise ValueError(f"{member_id} no puede invitar ese día")
    guest_name = check_name(guest_name)
    today = [g for g in store.guests if g["member"] == member_id and g["day"] == day.isoformat()]
    if len(today) >= PER_DAY:
        raise ValueError(f"{member_id} ya tiene {PER_DAY} invitados ese día")
    guest = {"member": member_id, "name": guest_name, "day": day.isoformat()}
    store.guests.append(guest)
    return guest


def guests_on(store, day):
    return [g for g in store.guests if g["day"] == day.isoformat()]


def guests_in_month(store, member_id, month):
    return [g for g in store.guests if g["member"] == member_id and g["day"][:7] == month]


def guests_text(store, day):
    found = guests_on(store, day)
    if not found:
        return "Sin invitados"
    return "\n".join(f"{g['name']} (invita {g['member']})" for g in found)


def busiest_month_day(store, month):
    """El día del mes con más invitados, o None."""
    counts = {}
    for guest in store.guests:
        if guest["day"][:7] == month:
            counts[guest["day"]] = counts.get(guest["day"], 0) + 1
    if not counts:
        return None
    return max(sorted(counts), key=lambda d: counts[d])


__all__ = ["invite", "guests_on", "guests_in_month", "guests_text", "busiest_month_day", "month_key"]
