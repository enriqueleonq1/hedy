"""Facturas: cada línea es precio x cantidad menos un descuento en porcentaje."""


def line_total(price, qty, discount_pct=0):
    """El importe de una línea, a 2 decimales."""
    total = price * qty * (1 - discount_pct / 100)
    return round(total, 2)


def invoice_total(lines):
    """La suma de las líneas, cada una ya redondeada."""
    return round(sum(line_total(**line) for line in lines), 2)
