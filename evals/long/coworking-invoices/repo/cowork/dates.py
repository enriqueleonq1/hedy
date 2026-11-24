"""Fechas y meses tal como los usa el coworking ("AAAA-MM", "HH:MM")."""

import calendar
from datetime import date, timedelta

MONTH_NAMES = [
    "enero", "febrero", "marzo", "abril", "mayo", "junio",
    "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre",
]


def parse_month(text):
    """'2025-03' -> (2025, 3). Lanza ValueError si no es un mes."""
    try:
        year, month = text.split("-")
        year, month = int(year), int(month)
    except ValueError:
        raise ValueError(f"Mes inválido: {text}") from None
    if not 1 <= month <= 12 or len(text) != 7:
        raise ValueError(f"Mes inválido: {text}")
    return year, month


def month_key(day):
    return f"{day.year:04d}-{day.month:02d}"


def month_name(text):
    """'2025-03' -> 'marzo 2025'."""
    year, month = parse_month(text)
    return f"{MONTH_NAMES[month - 1]} {year}"


def days_in_month(text):
    year, month = parse_month(text)
    return calendar.monthrange(year, month)[1]


def first_day(text):
    year, month = parse_month(text)
    return date(year, month, 1)


def last_day(text):
    year, month = parse_month(text)
    return date(year, month, days_in_month(text))


def days_of(text):
    """Cada día del mes, en orden."""
    day = first_day(text)
    end = last_day(text)
    while day <= end:
        yield day
        day += timedelta(days=1)


def parse_day(text):
    return date.fromisoformat(text)


def minutes(hhmm):
    """'09:30' -> 570."""
    hours, mins = hhmm.split(":")
    return int(hours) * 60 + int(mins)


def hhmm(total):
    return f"{total // 60:02d}:{total % 60:02d}"
