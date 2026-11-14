from datetime import date, timedelta


def business_days(start: date, end: date) -> int:
    """Días hábiles (lunes a viernes) entre start y end, ambos incluidos."""
    days = 0
    current = start + timedelta(days=1)
    while current <= end:
        if current.weekday() < 5:
            days += 1
        current += timedelta(days=1)
    return days
