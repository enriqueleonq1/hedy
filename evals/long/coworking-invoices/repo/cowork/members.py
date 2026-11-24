"""Socios: alta, búsqueda, listado y si estaban activos en un mes."""

from .dates import first_day, last_day
from .models import Member


class UnknownMember(KeyError):
    pass


def find_member(store, member_id):
    for member in store.members:
        if member.id == member_id:
            return member
    raise UnknownMember(member_id)


def list_members(store):
    return sorted(store.members, key=lambda m: m.id)


def add_member(store, member_id, name, email, nif, plan, joined):
    """Da de alta un socio. El plan tiene que existir y el id no puede estar usado."""
    if plan not in store.plans:
        raise ValueError(f"Plan desconocido: {plan}")
    if any(m.id == member_id for m in store.members):
        raise ValueError(f"Ya existe el socio {member_id}")
    member = Member(member_id, name, email, nif, plan, joined)
    store.members.append(member)
    return member


def change_plan(store, member_id, plan):
    if plan not in store.plans:
        raise ValueError(f"Plan desconocido: {plan}")
    member = find_member(store, member_id)
    member.plan = plan
    return member


def is_active(member, month):
    """Si fue socio algún día del mes ('AAAA-MM')."""
    if member.joined > last_day(month):
        return False
    return member.left is None or member.left >= first_day(month)


def active_members(store, month):
    return [m for m in list_members(store) if is_active(m, month)]


def members_text(store):
    """Una línea por socio: 'S001 Ana Ruiz (Fijo)'."""
    lines = []
    for member in list_members(store):
        plan = store.plans[member.plan].name
        lines.append(f"{member.id} {member.name} ({plan})")
    return "\n".join(lines)


def next_id(store):
    numbers = [int(m.id[1:]) for m in store.members if m.id[1:].isdigit()]
    return f"S{max(numbers, default=0) + 1:03d}"
