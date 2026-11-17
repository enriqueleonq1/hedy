"""Money is integer cents everywhere; these only format it."""


def fmt(cents: int) -> str:
    """For people: 1250 -> "$12.50"."""
    return f"${cents // 100}.{cents % 100:02d}"


def plain(cents: int) -> str:
    """For files: 1250 -> "12.50"."""
    return f"{cents // 100}.{cents % 100:02d}"
