"""Duraciones del formulario de pausas."""

import re

_DURATION = re.compile(r"(?:(\d+)h)?\s*(?:(\d+)m)?")


def format_minutes(minutes):
    """90 -> '1h30m'."""
    hours, rest = divmod(minutes, 60)
    if hours and rest:
        return f"{hours}h{rest}m"
    return f"{hours}h" if hours else f"{rest}m"


def parse_duration(text):
    """'1h30m', '1h 30m', '45m' o '2H' -> minutos. Lo que no se entiende: ValueError."""
    cleaned = text.strip().lower()
    match = _DURATION.fullmatch(cleaned)
    if not cleaned or not match or not any(match.groups()):
        raise ValueError(f"duración no válida: {text!r}")
    hours, minutes = match.groups()
    return int(hours or 0) * 60 + int(minutes or 0)
