from .coupons import discount
from .money import text as money
from .tax import tax




def totals(cart, region, coupon=None):
    subtotal = cart.subtotal()
    off = discount(subtotal, coupon)
    taxed = subtotal - off
    t = tax(taxed, region)
    return {"subtotal": subtotal, "discount": off, "tax": t, "total": taxed + t}


def invoice_text(cart, region, coupon=None):
    lines = [f"{qty} x {sku}: {money(total)}" for sku, qty, total in cart.line_totals()]
    t = totals(cart, region, coupon)
    lines.append(f"Subtotal: {money(t['subtotal'])}")
    if t["discount"]:
        lines.append(f"Descuento: -{money(t['discount'])}")
    lines.append(f"Impuesto: {money(t['tax'])}")
    lines.append(f"Total: {money(t['total'])}")
    return "\n".join(lines)
