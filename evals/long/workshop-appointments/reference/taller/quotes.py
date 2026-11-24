"""Presupuestos: mano de obra y repuestos de unos servicios, con IVA."""
from . import money
from .models import Quote, QuoteLine, Service


def lines_for(services: list[Service], inventory, extra_parts=()) -> list[QuoteLine]:
    """Una línea de mano de obra por servicio y una por repuesto (sumados por código)."""
    lines = [QuoteLine(f"Mano de obra: {s.name}", s.labor_cents) for s in services]
    qty: dict[str, int] = {}
    for s in services:
        for use in s.parts:
            qty[use.code] = qty.get(use.code, 0) + use.qty
    for use in extra_parts:
        qty[use.code] = qty.get(use.code, 0) + use.qty
    for code, n in qty.items():
        part = inventory.get(code)
        lines.append(QuoteLine(f"{n} x {part.name}", n * part.price_cents))
    return lines


FREQUENT_ORDERS = 3
FREQUENT_PERCENT = 10


def frequent_discount(lines: list[QuoteLine]) -> QuoteLine:
    """El descuento de cliente frecuente: un porcentaje de la mano de obra, en negativo."""
    labor = sum(l.cents for l in lines if l.description.startswith("Mano de obra:"))
    return QuoteLine(f"Descuento cliente frecuente ({FREQUENT_PERCENT}%)", -money.percent(labor, FREQUENT_PERCENT))


def build(plate: str, lines: list[QuoteLine], title: str = "") -> Quote:
    subtotal = sum(l.cents for l in lines)
    vat = money.vat(subtotal)
    return Quote(plate, lines, subtotal, vat, subtotal + vat, title or f"Presupuesto {plate}")


def text(quote: Quote) -> str:
    width = max([len(l.description) for l in quote.lines] + [10])
    out = [quote.title]
    for line in quote.lines:
        out.append(f"{line.description:<{width}}  {money.fmt(line.cents):>16}")
    out.append(f"{'Subtotal':<{width}}  {money.fmt(quote.subtotal_cents):>16}")
    out.append(f"{'IVA 21%':<{width}}  {money.fmt(quote.vat_cents):>16}")
    out.append(f"{'Total':<{width}}  {money.fmt(quote.total_cents):>16}")
    return "\n".join(out)
