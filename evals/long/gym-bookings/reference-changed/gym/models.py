"""Lo que maneja el gimnasio: clases, socios, reservas, pagos y mensajes."""
from dataclasses import dataclass
from datetime import date, datetime, timedelta


class GymError(Exception):
    """Algo que no se puede hacer. Su texto es el mensaje que ve el socio."""


@dataclass
class GymClass:
    id: str
    name: str
    start: datetime
    capacity: int
    instructor: str
    room: str = "Sala 1"
    minutes: int = 60

    @property
    def end(self):
        return self.start + timedelta(minutes=self.minutes)

    @property
    def day(self):
        return self.start.date()


@dataclass
class Member:
    id: str
    name: str
    email: str
    plan: str  # "basic" o "premium"
    family: str = ""


@dataclass
class Booking:
    id: int
    member_id: str
    class_id: str
    created_at: datetime
    status: str = "plaza"  # "plaza" o "espera"
    checked_in: bool = False


@dataclass
class Freeze:
    member_id: str
    start: date
    end: date

    def covers(self, day):
        return self.start <= day <= self.end


@dataclass
class PlanChange:
    member_id: str
    plan: str
    since: date


@dataclass
class Payment:
    member_id: str
    day: date
    cents: int
    note: str = ""


@dataclass
class Message:
    to: str
    subject: str
    body: str
    created_at: datetime
    sent: bool = False


def month_of(moment):
    """'2025-03' para cualquier fecha u hora de marzo de 2025."""
    return f"{moment.year:04d}-{moment.month:02d}"


def parse_month(text):
    """'2025-03' -> (2025, 3). Lo que no tiene esa forma es un error."""
    try:
        year, month = text.split("-")
        year, month = int(year), int(month)
    except ValueError:
        raise GymError(f"Mes inválido: {text}") from None
    if not 1 <= month <= 12 or len(text) != 7:
        raise GymError(f"Mes inválido: {text}")
    return year, month


def days_in_month(year, month):
    following = date(year + month // 12, month % 12 + 1, 1)
    return (following - date(year, month, 1)).days
