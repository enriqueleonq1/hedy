from .money import ratio

COUPONS = {"TEN": ("percent", 10), "SAVE5": ("fixed", 500), "HALF": ("percent", 50), "BIG": ("fixed", 10000)}


def discount(subtotal, code):
    if code is None:
        return 0
    kind, value = COUPONS[code]
    if kind == "percent":
        return ratio(subtotal, value, 100)
    return min(value, subtotal)
