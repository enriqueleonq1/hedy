"""Facturas del mes: el plan y los consumos de cada socio."""

from .extras import consumptions_in_month
from .members import find_member
from .models import Invoice, Line
from .money import total


def next_number(store):
    return f"F-{len(store.invoices) + 1:04d}"


def invoice(store, member_id, month):
    """Emite la factura del mes de un socio: el plan entero y una línea por consumo."""
    member = find_member(store, member_id)
    plan = store.plans[member.plan]
    lines = [Line(f"Plan {plan.name}", plan.price_cents)]
    for item in consumptions_in_month(store, member_id, month):
        extra = store.extras[item.extra]
        lines.append(Line(f"{extra.name} x {item.qty}", extra.price_cents * item.qty))
    result = Invoice(next_number(store), member_id, month, lines, total(lines))
    store.invoices.append(result)
    return result


def find_invoice(store, number):
    for item in store.invoices:
        if item.number == number:
            return item
    return None


def invoices_of(store, month):
    return [i for i in store.invoices if i.month == month]


def invoice_text(item):
    lines = [f"Factura {item.number} ({item.member_id}, {item.month})"]
    lines += [f"{line.description}: {line.cents}" for line in item.lines]
    lines.append(f"Total: {item.total_cents}")
    return "\n".join(lines)
