"""Reportes para la gerencia."""

import csv
import io

from .access import visits_in_month
from .billing import invoices_of, month_total
from .dates import month_key, month_name
from .extras import consumptions_in_month
from .members import active_members, list_members
from .money import eur, plain
from .rooms import duration, room_minutes


def hours_text(total_minutes):
    """750 -> '12,5 h'; 720 -> '12 h' (en medias horas)."""
    hours, rest = divmod(total_minutes, 60)
    return f"{hours} h" if rest == 0 else f"{hours},{rest * 10 // 60} h"


def month_report(store, month):
    """Resumen del mes en texto."""
    active = active_members(store, month)
    joined = [m for m in store.members if month_key(m.joined) == month]
    left = [m for m in store.members if m.left and month_key(m.left) == month]
    bookings = [b for b in store.bookings if month_key(b.day) == month and not b.cancelled]
    booked = sum(duration(b) for b in bookings)
    items = sum(len(consumptions_in_month(store, m.id, month)) for m in store.members)
    lines = [
        f"Resumen de {month_name(month)}",
        f"Socios activos: {len(active)}",
        f"Altas: {len(joined)}",
        f"Bajas: {len(left)}",
        f"Reservas: {len(bookings)} ({booked // 60} h {booked % 60:02d} min)",
        f"Consumos: {items}",
        f"Facturado: {eur(month_total(store, month))}",
    ]
    for room in store.rooms.values():
        lines.append(f"Ocupación {room.name}: {hours_text(room_minutes(store, room.id, month))}")
    return "\n".join(lines)


def invoices_csv(store, month):
    """Las facturas del mes y sus notas de crédito, para la gestoría."""
    out = io.StringIO()
    writer = csv.writer(out, lineterminator="\n")
    writer.writerow(["numero", "factura", "socio", "subtotal", "iva", "total"])
    items = invoices_of(store, month)
    for item in items:
        writer.writerow([
            item.number, "", item.member_id, plain(item.subtotal_cents),
            plain(item.vat_cents), plain(item.total_cents),
        ])
    by_number = {i.number: i for i in items}
    for note in store.credit_notes:
        if note.invoice in by_number:
            writer.writerow([
                note.number, note.invoice, by_number[note.invoice].member_id, "", "",
                plain(note.total_cents),
            ])
    return out.getvalue()


def attendance_text(store, month):
    """Días con entradas de cada socio activo en el mes."""
    lines = [f"Asistencia de {month_name(month)}"]
    for member in active_members(store, month):
        lines.append(f"{member.id} {member.name}: {visits_in_month(store, member.id, month)} días")
    return "\n".join(lines)


def members_csv(store):
    out = io.StringIO()
    writer = csv.writer(out, lineterminator="\n")
    writer.writerow(["id", "nombre", "email", "plan", "alta", "baja"])
    for member in list_members(store):
        writer.writerow([
            member.id, member.name, member.email, member.plan,
            member.joined.isoformat(), member.left.isoformat() if member.left else "",
        ])
    return out.getvalue()


def plan_mix_text(store, month):
    """Cuántos socios activos hay en cada plan ese mes."""
    counts = {plan_id: 0 for plan_id in store.plans}
    for member in active_members(store, month):
        counts[member.plan] += 1
    lines = [f"Planes en {month_name(month)}"]
    for plan_id, count in counts.items():
        lines.append(f"  {store.plans[plan_id].name}: {count}")
    return "\n".join(lines)


def extras_ranking(store, month):
    """Los extras del mes, del más pedido al menos, como (nombre, unidades)."""
    units = {}
    for item in store.consumptions:
        if month_key(item.day) == month:
            units[item.extra] = units.get(item.extra, 0) + item.qty
    ranking = sorted(units.items(), key=lambda pair: (-pair[1], pair[0]))
    return [(store.extras[code].name, qty) for code, qty in ranking]


def extras_ranking_text(store, month):
    ranking = extras_ranking(store, month)
    if not ranking:
        return "Sin consumos"
    return "\n".join(f"{name}: {qty}" for name, qty in ranking)
