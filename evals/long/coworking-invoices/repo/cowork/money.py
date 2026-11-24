"""Dinero en centavos enteros."""


def fmt(cents):
    """1234 -> 'EUR 12.34' (formato de los reportes internos)."""
    sign = "-" if cents < 0 else ""
    cents = abs(cents)
    return f"{sign}EUR {cents // 100}.{cents % 100:02d}"


def ratio(amount, num, den):
    """amount * num / den, redondeado al centavo con la mitad hacia arriba
    (para montos positivos)."""
    return (amount * num * 2 + den) // (2 * den)


def total(lines):
    return sum(line.cents for line in lines)
