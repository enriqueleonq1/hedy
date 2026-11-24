def ratio(amount, num, den):
    """amount * num / den in whole cents, half up (amounts are never negative)."""
    return (amount * num * 2 + den) // (2 * den)


def text(cents):
    whole, rest = divmod(cents, 100)
    return f"${whole:,}.{rest:02d}"
