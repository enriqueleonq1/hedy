"""El horario semanal y sus choques (sala o instructor ocupados dos veces)."""
from datetime import timedelta

from .catalog import DAYS, by_start
from .validation import check_class


def week_of(day):
    """El lunes de la semana de `day`."""
    return day - timedelta(days=day.weekday())


def week_classes(classes, day):
    monday = week_of(day)
    sunday = monday + timedelta(days=6)
    return by_start(c for c in classes if monday <= c.day <= sunday)


def overlaps(a, b):
    return a.start < b.end and b.start < a.end


def conflicts(classes):
    """Pares de clases que se pisan en la misma sala o con el mismo instructor."""
    found = []
    ordered = by_start(classes)
    for i, a in enumerate(ordered):
        for b in ordered[i + 1 :]:
            if b.start >= a.end:
                break
            if not overlaps(a, b):
                continue
            if a.room == b.room:
                found.append((a, b, f"misma sala ({a.room})"))
            if a.instructor == b.instructor:
                found.append((a, b, f"mismo instructor ({a.instructor})"))
    return found


def week_text(classes, day):
    """El horario de la semana de `day`, día por día."""
    monday = week_of(day)
    rows = [f"Semana del {monday:%d/%m/%Y}"]
    for offset in range(7):
        current = monday + timedelta(days=offset)
        todays = [c for c in week_classes(classes, day) if c.day == current]
        if not todays:
            continue
        rows.append(f"{DAYS[current.weekday()]} {current:%d/%m}")
        for c in todays:
            rows.append(f"  {c.start:%H:%M}–{c.end:%H:%M} {c.name} · {c.instructor} · {c.room}")
    if len(rows) == 1:
        rows.append("Sin clases")
    return "\n".join(rows)


def review(classes):
    """Todo lo que está mal en las clases: datos inválidos y choques."""
    problems = []
    for gym_class in by_start(classes):
        problems.extend(check_class(gym_class))
    for a, b, why in conflicts(classes):
        problems.append(f"{a.id} y {b.id} se pisan: {why}")
    return problems


def hours_by_instructor(classes):
    """Horas de clase de cada instructor, {'Marta': 3.0, …}."""
    hours = {}
    for c in classes:
        hours[c.instructor] = hours.get(c.instructor, 0) + c.minutes / 60
    return dict(sorted(hours.items()))
