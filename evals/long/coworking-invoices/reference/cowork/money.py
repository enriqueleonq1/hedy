"""Dinero en centavos enteros."""


def eur(cents):
    """123456 -> '1.234,56 €'; -2500 -> '-25,00 €'."""
    sign = "-" if cents < 0 else ""
    whole, rest = divmod(abs(cents), 100)
    return f"{sign}{whole:,}".replace(",", ".") + f",{rest:02d} €"


def plain(cents):
    """Euros con punto decimal, para archivos: 23940 -> '239.40'."""
    sign = "-" if cents < 0 else ""
    whole, rest = divmod(abs(cents), 100)
    return f"{sign}{whole}.{rest:02d}"


def ratio(amount, num, den):
    """amount * num / den, redondeado al centavo con la mitad hacia arriba
    (en valor absoluto: -2,5 es -3)."""
    if amount < 0:
        return -ratio(-amount, num, den)
    return (amount * num * 2 + den) // (2 * den)


def total(lines):
    return sum(line.cents for line in lines)


# Lo que ya usaban los reportes internos: ahora es el mismo formato.
fmt = eur
