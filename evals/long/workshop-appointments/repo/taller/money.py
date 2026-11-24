"""Importes en centavos enteros y cómo se muestran."""

VAT_PERCENT = 21


def fmt(cents: int) -> str:
    """1234550 -> '$ 12.345,50' (punto de miles, coma decimal)."""
    sign = "-" if cents < 0 else ""
    whole, rest = divmod(abs(cents), 100)
    thousands = f"{whole:,}".replace(",", ".")
    return f"{sign}$ {thousands},{rest:02d}"


def percent(cents: int, pct: int) -> int:
    """pct % de cents, redondeado con la mitad hacia arriba (para importes positivos)."""
    return (cents * pct * 2 + 100) // 200


def vat(cents: int) -> int:
    return percent(cents, VAT_PERCENT)


def parse(text: str) -> int:
    """'12.345,50' o '12345.50' -> centavos."""
    text = text.strip().replace("$", "").strip()
    if "," in text:
        text = text.replace(".", "").replace(",", ".")
    whole, _, frac = text.partition(".")
    frac = (frac + "00")[:2]
    return int(whole or "0") * 100 + int(frac)
