"""Money is kept in integer cents and only formatted for display."""
from library.config import CURRENCY_SYMBOL


def fmt_cents(cents: int) -> str:
    """1250 -> "$12.50"; negative amounts keep the sign in front."""
    sign = "-" if cents < 0 else ""
    cents = abs(cents)
    return f"{sign}{CURRENCY_SYMBOL}{cents // 100}.{cents % 100:02d}"


def parse_cents(text: str) -> int:
    """"12.50" or "12,50" -> 1250. Empty text is zero."""
    text = text.strip().replace(CURRENCY_SYMBOL, "").replace(",", ".")
    if not text:
        return 0
    whole, _, fraction = text.partition(".")
    fraction = (fraction + "00")[:2]
    sign = -1 if whole.startswith("-") else 1
    return sign * (abs(int(whole or "0")) * 100 + int(fraction))
