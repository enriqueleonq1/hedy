"""Historial de servicios de cada vehículo y cuándo le toca el próximo."""
from datetime import date

# Cada cuántos meses se recomienda repetir un servicio (los que no están, no se repiten).
EVERY_MONTHS = {"OIL": 6, "ALI": 12, "BRK": 18}


def add_months(day: date, months: int) -> date:
    month = day.month - 1 + months
    year = day.year + month // 12
    month = month % 12 + 1
    last = [31, 29 if year % 4 == 0 and (year % 100 or year % 400 == 0) else 28,
            31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month - 1]
    return date(year, month, min(day.day, last))


def services_done(orders, plate: str) -> list[tuple[str, str]]:
    """(día de cierre, código) de cada servicio hecho al vehículo, del más viejo al más nuevo."""
    done = []
    for o in orders.of_plate(plate):
        if o.status == "cerrada":
            for code in o.services:
                done.append((o.closed, code))
    return sorted(done)


def next_due(orders, plate: str) -> dict[str, date]:
    """Para cada servicio que se repite, el día en que le toca de nuevo."""
    last: dict[str, str] = {}
    for closed, code in services_done(orders, plate):
        last[code] = closed
    return {
        code: add_months(date.fromisoformat(day), EVERY_MONTHS[code])
        for code, day in last.items()
        if code in EVERY_MONTHS
    }


def history_text(book, catalog, orders, plate: str) -> str:
    vehicle = book.vehicle(plate)
    lines = [f"{vehicle.plate} — {vehicle.label()}"]
    done = services_done(orders, vehicle.plate)
    if not done:
        lines.append("  sin servicios")
    for closed, code in done:
        lines.append(f"  {closed} {catalog.get(code).name}")
    for code, day in sorted(next_due(orders, vehicle.plate).items(), key=lambda kv: kv[1]):
        lines.append(f"  próximo {catalog.get(code).name}: {day.isoformat()}")
    return "\n".join(lines)
