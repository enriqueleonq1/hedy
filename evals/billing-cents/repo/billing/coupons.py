# A coupon is ("percent", 10) for 10 % off, or ("fixed", 5.0) for 5 dollars off.
COUPONS = {
    "TEN": ("percent", 10),
    "SAVE5": ("fixed", 5.0),
    "HALF": ("percent", 50),
    "BIG": ("fixed", 100.0),
}


def discount(subtotal, code):
    if code is None:
        return 0.0
    kind, value = COUPONS[code]
    if kind == "percent":
        return round(subtotal * value / 100, 2)
    return min(value, subtotal)
