"""Reportes en texto para el mostrador y la administración."""
from . import money

MONTHS = ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto",
          "septiembre", "octubre", "noviembre", "diciembre"]


def month_name(month: str) -> str:
    """'2025-03' -> 'marzo 2025'."""
    year, number = month.split("-")
    return f"{MONTHS[int(number) - 1]} {year}"


def customers_text(book) -> str:
    lines = []
    for c in book.all():
        plates = ", ".join(v.plate for v in book.vehicles_of(c.id)) or "sin vehículos"
        lines.append(f"{c.id}  {c.name} ({plates})")
    return "\n".join(lines)


def vehicles_text(book) -> str:
    return "\n".join(f"{v.plate}  {v.label()}  {book.get(v.customer).name}" for v in book.vehicles())


def mechanics_text(staff) -> str:
    return "\n".join(f"{m.id}  {m.name}: {', '.join(m.skills)}" for m in staff.all())


def services_text(catalog) -> str:
    return "\n".join(
        f"{s.code:<4} {s.name} ({s.minutes} min, {s.skill}) {money.fmt(s.labor_cents)}" for s in catalog.all()
    )


def parts_text(parts) -> str:
    return "\n".join(f"{p.code:<9} {p.name}: {p.stock} (mín. {p.min_stock}) {money.fmt(p.price_cents)}" for p in parts)


def orders_text(orders) -> str:
    lines = []
    for o in orders:
        state = f"cerrada el {o.closed}" if o.status == "cerrada" else "abierta"
        total = money.fmt(o.total_cents) if o.status == "cerrada" else "—"
        lines.append(f"#{o.id} {o.plate} {'+'.join(o.services)} {state} {total}")
    return "\n".join(lines)


def monthly_text(orders, month: str) -> str:
    """Lo facturado en el mes: una línea por orden cerrada y el total."""
    closed = orders.closed_in(month)
    lines = [f"Facturación de {month_name(month)}"]
    for o in closed:
        lines.append(f"  #{o.id} {o.closed} {o.plate} {money.fmt(o.total_cents)}")
    total = sum(o.total_cents for o in closed)
    lines.append(f"Total: {money.fmt(total)} en {len(closed)} órdenes")
    return "\n".join(lines)


def parts_usage(orders, month: str) -> dict[str, int]:
    """Unidades de cada repuesto usadas en las órdenes cerradas del mes."""
    used: dict[str, int] = {}
    for o in orders.closed_in(month):
        for use in o.parts:
            used[use.code] = used.get(use.code, 0) + use.qty
    return used


def mechanics_month_text(staff, orders, month: str) -> str:
    """Órdenes cerradas por cada mecánico en el mes, en el orden del archivo."""
    lines = [f"Órdenes por mecánico, {month_name(month)}"]
    closed = orders.closed_in(month)
    for m in staff.all():
        mine = [o for o in closed if o.mechanic == m.id]
        total = sum(o.total_cents for o in mine)
        lines.append(f"  {m.name}: {len(mine)} órdenes, {money.fmt(total)}")
    return "\n".join(lines)


def day_text(staff, catalog, appointments, day) -> str:
    """La agenda del día: cada mecánico, en el orden del archivo, con sus turnos por hora."""
    lines = [f"Agenda del taller — {day.strftime('%d/%m/%Y')}"]
    for m in staff.all():
        lines.append(f"{m.name}:")
        mine = [a for a in appointments if a.mechanic == m.id and a.active]
        if not mine:
            lines.append("  (libre)")
        for a in sorted(mine, key=lambda a: a.start):
            lines.append(f"  {a.start}–{a.end} {a.plate} {catalog.get(a.service).name}")
    return "\n".join(lines)
