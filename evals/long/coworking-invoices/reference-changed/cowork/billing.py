"""Facturas del mes, notas de crédito y cobros."""

from .dates import days_in_month, month_name, parse_month
from .extras import consumptions_in_month
from .members import DISCOUNTS, active_days, find_member, is_active, list_members
from .models import CreditNote, Invoice, Line
from .money import eur, ratio, total
from .rooms import member_minutes

VAT_PCT = 21
# Cada hora extra de sala empezada, para todos los planes.
EXTRA_HOUR_CENTS = 1500


def invoice_number(store, month):
    """F-<año del mes>-<4 cifras>: cada año empieza su serie en 0001."""
    year, _ = parse_month(month)
    same_year = [i for i in store.invoices if parse_month(i.month)[0] == year]
    return f"F-{year}-{len(same_year) + 1:04d}"


def note_number(store, invoice):
    year, _ = parse_month(invoice.month)
    return f"NC-{year}-{len(store.credit_notes) + 1:04d}"


def vat(lines):
    """La suma del 21 % de cada línea, cada uno redondeado al centavo con la
    mitad hacia arriba en valor absoluto (-787,5 es -788)."""
    return sum(ratio(line.cents, VAT_PCT, 100) for line in lines)


def plan_line(store, member, month):
    plan = store.plans[member.plan]
    description = f"Plan {plan.name} — {month_name(month)}"
    days, all_days = active_days(member, month), days_in_month(month)
    if days == all_days:
        return Line(description, plan.price_cents)
    return Line(f"{description} ({days} de {all_days} días)", ratio(plan.price_cents, days, all_days))


def discount_line(member, plan):
    percent = DISCOUNTS[member.discount]
    return Line(f"Descuento {member.discount} ({percent}%)", -ratio(plan.cents, percent, 100))


def extra_hours_line(store, member, month):
    included = store.plans[member.plan].room_hours * 60
    over = member_minutes(store, member.id, month) - included
    if over <= 0:
        return None
    hours = -(-over // 60)
    return Line(f"Horas extra de sala: {hours} h", hours * EXTRA_HOUR_CENTS)


def consumption_lines(store, member_id, month):
    """Un línea por extra consumido en el mes, en el orden del catálogo."""
    qty = {}
    for item in consumptions_in_month(store, member_id, month):
        qty[item.extra] = qty.get(item.extra, 0) + item.qty
    return [
        Line(f"{extra.name} x {qty[code]}", extra.price_cents * qty[code])
        for code, extra in store.extras.items()
        if code in qty
    ]


def invoice_lines(store, member, month):
    plan = plan_line(store, member, month)
    lines = [plan]
    if member.discount:
        lines.append(discount_line(member, plan))
    hours = extra_hours_line(store, member, month)
    if hours:
        lines.append(hours)
    return lines + consumption_lines(store, member.id, month)


def invoice(store, member_id, month):
    """Emite la factura del mes de un socio y deja su correo en la bandeja."""
    from .messages import send_invoice

    member = find_member(store, member_id)
    if not is_active(member, month):
        raise ValueError(f"{member_id} no estuvo activo en {month}")
    if any(i.member_id == member_id and i.month == month for i in store.invoices):
        raise ValueError(f"Ya existe una factura de {member_id} para {month}")
    lines = invoice_lines(store, member, month)
    subtotal, tax = total(lines), vat(lines)
    item = Invoice(
        number=invoice_number(store, month),
        member_id=member_id,
        month=month,
        lines=lines,
        total_cents=subtotal + tax,
        subtotal_cents=subtotal,
        vat_cents=tax,
    )
    store.invoices.append(item)
    send_invoice(store, item)
    return item


def invoice_all(store, month):
    """Factura a los socios activos del mes que aún no tienen factura, por id."""
    done = {i.member_id for i in store.invoices if i.month == month}
    return [
        invoice(store, member.id, month)
        for member in list_members(store)
        if is_active(member, month) and member.id not in done
    ]


def find_invoice(store, number):
    for item in store.invoices:
        if item.number == number:
            return item
    return None


def get_invoice(store, number):
    item = find_invoice(store, number)
    if item is None:
        raise ValueError(f"No existe la factura {number}")
    return item


def invoices_of(store, month):
    return [i for i in store.invoices if i.month == month]


def notes_on(store, number):
    return [n for n in store.credit_notes if n.invoice == number]


def credit_note(store, number, cents, reason):
    """Una nota de crédito de `cents` sobre una factura."""
    item = get_invoice(store, number)
    if cents <= 0:
        raise ValueError("La nota tiene que ser de un importe positivo")
    credited = -sum(n.total_cents for n in notes_on(store, number))
    if credited + cents > item.total_cents:
        raise ValueError(f"La nota supera lo facturado en {number}")
    note = CreditNote(note_number(store, item), number, -cents, reason)
    store.credit_notes.append(note)
    return note


def month_total(store, month):
    """Lo facturado en el mes, menos las notas sobre sus facturas."""
    items = invoices_of(store, month)
    numbers = {i.number for i in items}
    notes = sum(n.total_cents for n in store.credit_notes if n.invoice in numbers)
    return sum(i.total_cents for i in items) + notes


def mark_paid(store, number, day):
    item = get_invoice(store, number)
    item.paid_on = day
    return item


def pending(store):
    """Las facturas sin cobrar, en orden de emisión."""
    return [i.number for i in store.invoices if i.paid_on is None]


def invoice_text(store, item):
    """El texto de la factura, tal como lo recibe el socio."""
    name = find_member(store, item.member_id).name
    lines = [
        f"Factura {item.number}",
        f"Socio: {name} ({item.member_id})",
        f"Periodo: {month_name(item.month)}",
    ]
    lines += [f"{line.description}: {eur(line.cents)}" for line in item.lines]
    lines += [
        f"Subtotal: {eur(item.subtotal_cents)}",
        f"IVA {VAT_PCT}%: {eur(item.vat_cents)}",
        f"Total: {eur(item.total_cents)}",
    ]
    return "\n".join(lines)


def credit_note_text(note):
    return "\n".join([
        f"Nota de crédito {note.number}",
        f"Factura: {note.invoice}",
        f"Motivo: {note.reason}",
        f"Total: {eur(note.total_cents)}",
    ])
