"""Altas y consultas de socios, congelamientos y cambios de plan."""
from datetime import date

from .models import Freeze, GymError, Member, PlanChange
from .validation import check_email, check_member_id, check_name, check_plan


def add_member(state, member_id, name, email, plan, family=""):
    """Da de alta un socio y lo devuelve."""
    member_id = check_member_id(member_id, state.members)
    name = check_name(name)
    plan = check_plan(plan)
    member = Member(
        id=member_id,
        name=name,
        email=check_email(email, state.members),
        plan=plan,
        family=family.strip(),
    )
    state.members[member.id] = member
    return member


def get_member(state, member_id):
    try:
        return state.members[member_id]
    except KeyError:
        raise GymError(f"Socio inexistente: {member_id}") from None


def format_member(member):
    """'Ana Pérez (premium) · ana@example.com'."""
    text = f"{member.name} ({member.plan})"
    if member.email:
        text += f" · {member.email}"
    return text


def by_plan(state, plan):
    return sorted((m for m in state.members.values() if m.plan == plan), key=lambda m: m.name)


def search(state, text):
    """Socios cuyo nombre o email contienen `text` (sin distinguir mayúsculas)."""
    text = text.lower()
    return sorted(
        (m for m in state.members.values() if text in m.name.lower() or text in m.email),
        key=lambda m: m.name,
    )


def family_members(state, family):
    if not family:
        return []
    return sorted((m for m in state.members.values() if m.family == family), key=lambda m: m.id)


# Cambios de plan


def first_of_next_month(day):
    return date(day.year + day.month // 12, day.month % 12 + 1, 1)


def change_plan(state, member_id, plan, now):
    """El socio pasa a `plan` desde el primer día del mes siguiente."""
    member = get_member(state, member_id)
    plan = check_plan(plan)
    since = first_of_next_month(now.date() if hasattr(now, "date") else now)
    state.plan_changes = [
        c for c in state.plan_changes if not (c.member_id == member.id and c.since == since)
    ]
    change = PlanChange(member.id, plan, since)
    state.plan_changes.append(change)
    return change


def plan_on(state, member_id, day):
    """El plan del socio en `day`."""
    member = get_member(state, member_id)
    plan = member.plan
    for change in sorted(state.plan_changes, key=lambda c: c.since):
        if change.member_id == member.id and change.since <= day:
            plan = change.plan
    return plan


# Congelamientos


def freeze(state, member_id, start, end):
    member = get_member(state, member_id)
    if end < start:
        raise GymError("El congelamiento termina antes de empezar")
    period = Freeze(member.id, start, end)
    state.freezes.append(period)
    return period


def freeze_on(state, member_id, day):
    """El congelamiento del socio que cubre `day`, si hay uno."""
    for period in state.freezes:
        if period.member_id == member_id and period.covers(day):
            return period
    return None


def frozen_days(state, member_id, year, month, days):
    """Cuántos días del mes estuvo congelado el socio."""
    count = 0
    for number in range(1, days + 1):
        if freeze_on(state, member_id, date(year, month, number)):
            count += 1
    return count
