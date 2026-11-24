"""Cuotas de los planes e IVA. Todo en centavos enteros."""

# Cuota mensual de cada plan, IVA aparte.
PLANS = {"basic": 2990, "premium": 4990}

VAT_PERCENT = 21


def plan_fee(plan):
    return PLANS[plan]


def percent_of(cents, percent):
    """`percent` % de `cents`, redondeado con la mitad hacia arriba."""
    return (cents * percent * 2 + 100) // 200


def vat_of(cents):
    return percent_of(cents, VAT_PERCENT)


def money(cents):
    """1234 -> '12,34 €'; -500 -> '-5,00 €'; 123456 -> '1.234,56 €'."""
    sign = "-" if cents < 0 else ""
    whole, rest = divmod(abs(cents), 100)
    return f"{sign}{whole:,}".replace(",", ".") + f",{rest:02d} €"
