"""Socios: alta, búsqueda, listado y si estaban activos en un mes."""

import re

from .dates import first_day, last_day
from .models import Member

EMAIL = re.compile(r"^[^@\s]+@[^@\s]+\.[^@\s]+$")
NIF_LETTERS = "TRWAGMYFPDXBNJZSQVHLCKE"
DISCOUNTS = {"AMIGO10": 10, "ANUAL15": 15}


class UnknownMember(KeyError):
    pass


def find_member(store, member_id):
    for member in store.members:
        if member.id == member_id:
            return member
    raise UnknownMember(member_id)


def list_members(store):
    return sorted(store.members, key=lambda m: m.id)


def check_email(email):
    if not EMAIL.match(email or ""):
        raise ValueError(f"Email inválido: {email}")
    return email


def check_nif(nif):
    valid = (
        isinstance(nif, str)
        and len(nif) == 9
        and nif[:8].isdigit()
        and nif[8] == NIF_LETTERS[int(nif[:8]) % 23]
    )
    if not valid:
        raise ValueError(f"NIF inválido: {nif}")
    return nif


def add_member(store, member_id, name, email, nif, plan, joined):
    """Da de alta un socio. El plan tiene que existir, el id no puede estar
    usado y el email y el NIF tienen que ser válidos."""
    check_email(email)
    check_nif(nif)
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


def set_discount(store, member_id, code):
    """Guarda el código de descuento de un socio (None lo quita)."""
    if code is not None and code not in DISCOUNTS:
        raise ValueError(f"Código de descuento desconocido: {code}")
    member = find_member(store, member_id)
    member.discount = code
    return member


def deactivate(store, member_id, day):
    """Da de baja a un socio ese día, sin borrarlo."""
    member = find_member(store, member_id)
    if day < member.joined:
        raise ValueError(f"{member_id} no puede darse de baja antes de su alta")
    member.left = day
    return member


def active_days(member, month):
    """Días del mes ('AAAA-MM') en que fue socio, contando el de alta y el de baja."""
    start = max(member.joined, first_day(month))
    end = min(member.left or last_day(month), last_day(month))
    return max((end - start).days + 1, 0)


def is_active(member, month):
    """Si fue socio algún día del mes ('AAAA-MM')."""
    if member.joined > last_day(month):
        return False
    return member.left is None or member.left >= first_day(month)


def active_members(store, month):
    return [m for m in list_members(store) if is_active(m, month)]


def members_text(store):
    """Una línea por socio: 'S001 Ana Ruiz (Fijo)', y la baja si la tiene:
    'S004 Diego Paz (Fijo) — baja 28/02/2025'."""
    lines = []
    for member in list_members(store):
        plan = store.plans[member.plan].name
        line = f"{member.id} {member.name} ({plan})"
        if member.left:
            line += f" — baja {member.left.strftime('%d/%m/%Y')}"
        lines.append(line)
    return "\n".join(lines)


def next_id(store):
    numbers = [int(m.id[1:]) for m in store.members if m.id[1:].isdigit()]
    return f"S{max(numbers, default=0) + 1:03d}"
