def normalize(phone: str) -> str:
    """+58 412 555 0101 -> +584125550101"""
    digits = "".join(c for c in phone if c.isdigit())
    return f"+{digits}"
