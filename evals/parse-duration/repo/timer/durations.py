"""Duraciones del formulario de pausas."""


def format_minutes(minutes):
    """90 -> '1h30m'."""
    hours, rest = divmod(minutes, 60)
    if hours and rest:
        return f"{hours}h{rest}m"
    return f"{hours}h" if hours else f"{rest}m"
