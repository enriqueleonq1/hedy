"""Reportes para la recepción."""
from .bookings import bookings_of_class, waiting_of_class
from .billing import invoice, paid_in
from .catalog import classes_on, in_month, instructors
from .models import parse_month
from .pricing import money


def occupancy(state, day):
    """Una línea por clase del día: '07:30 Spinning (Raúl) 18/20', con
    ' · 3 en espera' si hay lista de espera."""
    lines = [f"Ocupación del {day:%d/%m/%Y}"]
    classes = classes_on(state.classes.values(), day)
    if not classes:
        lines.append("Sin clases")
    for gym_class in classes:
        taken = len(bookings_of_class(state, gym_class.id))
        line = (
            f"{gym_class.start:%H:%M} {gym_class.name} ({gym_class.instructor}) "
            f"{taken}/{gym_class.capacity}"
        )
        waiting = len(waiting_of_class(state, gym_class.id))
        if waiting:
            line += f" · {waiting} en espera"
        lines.append(line)
    return "\n".join(lines)


def instructor_report(state, month):
    """Por instructor: clases del mes y porcentaje de asistencia (check-ins
    sobre plazas reservadas, con la mitad hacia arriba)."""
    year, number = parse_month(month)
    classes = in_month(state.classes.values(), year, number)
    rows = ["Instructor;Clases;Asistencia"]
    for name in instructors(classes):
        theirs = [c for c in classes if c.instructor == name]
        places = [b for c in theirs for b in bookings_of_class(state, c.id)]
        if places:
            came = sum(1 for b in places if b.checked_in)
            attendance = f"{(came * 200 + len(places)) // (2 * len(places))}%"
        else:
            attendance = "—"
        rows.append(f"{name};{len(theirs)};{attendance}")
    return "\n".join(rows)


def revenue(state, month):
    """Lo facturado y lo cobrado del mes."""
    billed = sum(invoice(state, m, month).total for m in state.members)
    collected = sum(paid_in(state, m, month) for m in state.members)
    return "\n".join(
        [
            f"Ingresos de {month}",
            f"Facturado: {money(billed)}",
            f"Cobrado: {money(collected)}",
            f"Pendiente: {money(billed - collected)}",
        ]
    )


def busiest(state, month):
    """Las clases del mes de más a menos reservas."""
    year, number = parse_month(month)
    classes = in_month(state.classes.values(), year, number)
    ranked = sorted(classes, key=lambda c: (-len(bookings_of_class(state, c.id)), c.start))
    return [(c, len(bookings_of_class(state, c.id))) for c in ranked]


def members_by_plan(state):
    """'basic: 3', 'premium: 2'…"""
    counts = {}
    for member in state.members.values():
        counts[member.plan] = counts.get(member.plan, 0) + 1
    return "\n".join(f"{plan}: {counts[plan]}" for plan in sorted(counts))
