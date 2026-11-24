"""Cuotas de los planes e IVA. Todo en centavos enteros."""

# Cuota mensual de cada plan, IVA aparte.
PLANS = {"basic": 2990, "premium": 4990, "pack": 0}

# Plan pack: cada clase a la que asistió (con check-in).
PACK_CLASS = 600
# Basic y pack: cada clase reservada con plaza a la que no fue.
NO_SHOW_FEE = 300
NO_SHOW_PLANS = ("basic", "pack")
FAMILY_DISCOUNT_PERCENT = 10

# Cuántas reservas con plaza activas puede tener cada plan (None: sin límite).
ACTIVE_LIMIT = {"basic": 4, "premium": None, "pack": None}

VAT_PERCENT = 21


def plan_fee(plan):
    return PLANS[plan]


def percent_of(cents, percent):
    """`percent` % de `cents`, redondeado con la mitad hacia arriba."""
    return (cents * percent * 2 + 100) // 200


def vat_of(cents):
    return percent_of(cents, VAT_PERCENT)


def prorate(cents, days, of_days):
    """`cents` por `days` de `of_days`, con la mitad hacia arriba."""
    return (cents * days * 2 + of_days) // (2 * of_days)


def money(cents):
    """1234 -> '12,34 €'; -500 -> '-5,00 €'; 123456 -> '1.234,56 €'."""
    sign = "-" if cents < 0 else ""
    whole, rest = divmod(abs(cents), 100)
    return f"{sign}{whole:,}".replace(",", ".") + f",{rest:02d} €"
