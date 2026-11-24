"""Planes del coworking."""

from .money import eur as fmt


def find_plan(store, plan_id):
    try:
        return store.plans[plan_id]
    except KeyError:
        raise ValueError(f"Plan desconocido: {plan_id}") from None


def plans_text(store):
    """Una línea por plan, con su precio y las horas de sala que incluye."""
    lines = []
    for plan in store.plans.values():
        hours = f"{plan.room_hours} h de sala" if plan.room_hours else "sin sala"
        lines.append(f"{plan.name}: {fmt(plan.price_cents)} al mes, {hours}")
    return "\n".join(lines)


def members_by_plan(store):
    """Cuántos socios hay en cada plan (incluidos los dados de baja)."""
    counts = {plan_id: 0 for plan_id in store.plans}
    for member in store.members:
        counts[member.plan] = counts.get(member.plan, 0) + 1
    return counts
