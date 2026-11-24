"""Planillas para la administración, en CSV (texto)."""
import csv
import io

from .bookings import bookings_of_class
from .catalog import by_start


def _csv(header, rows):
    buffer = io.StringIO()
    writer = csv.writer(buffer, lineterminator="\n")
    writer.writerow(header)
    writer.writerows(rows)
    return buffer.getvalue()


def members_csv(state):
    rows = [
        (m.id, m.name, m.email, m.plan, m.family)
        for m in sorted(state.members.values(), key=lambda m: m.id)
    ]
    return _csv(["id", "nombre", "email", "plan", "familia"], rows)


def classes_csv(state):
    rows = []
    for c in by_start(state.classes.values()):
        rows.append(
            (
                c.id,
                c.name,
                c.start.isoformat(timespec="minutes"),
                c.instructor,
                c.room,
                c.capacity,
                len(bookings_of_class(state, c.id)),
            )
        )
    return _csv(["id", "clase", "inicio", "instructor", "sala", "capacidad", "reservas"], rows)


def payments_csv(state, month=None):
    rows = []
    for p in sorted(state.payments, key=lambda p: (p.day, p.member_id)):
        if month and p.day.strftime("%Y-%m") != month:
            continue
        rows.append((p.day.isoformat(), p.member_id, f"{p.cents / 100:.2f}", p.note))
    return _csv(["fecha", "socio", "importe", "nota"], rows)
