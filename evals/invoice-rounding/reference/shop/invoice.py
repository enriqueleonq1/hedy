"""Facturas: cada línea es precio x cantidad menos un descuento en porcentaje."""

from decimal import ROUND_HALF_UP, Decimal

CENT = Decimal("0.01")


def line_total(price, qty, discount_pct=0):
    """El importe de una línea, a 2 decimales con la mitad hacia arriba."""
    total = Decimal(str(price)) * Decimal(str(qty)) * (1 - Decimal(str(discount_pct)) / 100)
    return total.quantize(CENT, rounding=ROUND_HALF_UP)


def invoice_total(lines):
    """La suma de las líneas, cada una ya redondeada."""
    return sum((line_total(**line) for line in lines), Decimal("0.00"))
