"""Consultas sobre las clases."""
from datetime import timedelta

DAYS = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"]


def format_class(gym_class):
    """'Yoga · lun 10/03 18:00 · Marta (15 plazas)'."""
    start = gym_class.start
    return (
        f"{gym_class.name} · {DAYS[start.weekday()]} {start:%d/%m %H:%M} · "
        f"{gym_class.instructor} ({gym_class.capacity} plazas)"
    )


def short_when(gym_class):
    """'10/03 a las 18:00'."""
    return f"{gym_class.start:%d/%m} a las {gym_class.start:%H:%M}"


def by_start(classes):
    return sorted(classes, key=lambda c: (c.start, c.name))


def classes_on(classes, day):
    """Las clases de un día, por hora."""
    return by_start(c for c in classes if c.day == day)


def by_instructor(classes, instructor):
    return by_start(c for c in classes if c.instructor == instructor)


def instructors(classes):
    return sorted({c.instructor for c in classes})


def upcoming(classes, now, hours=24):
    """Las clases que empiezan entre `now` y `now` + `hours`."""
    limit = now + timedelta(hours=hours)
    return by_start(c for c in classes if now <= c.start < limit)


def in_month(classes, year, month):
    return by_start(c for c in classes if c.start.year == year and c.start.month == month)
