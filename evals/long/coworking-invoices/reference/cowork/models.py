"""Los datos del coworking. Los montos van siempre en centavos enteros."""

from dataclasses import dataclass, field
from datetime import date
from typing import Optional


@dataclass
class Plan:
    id: str
    name: str
    price_cents: int
    # Horas de sala incluidas cada mes.
    room_hours: int


@dataclass
class Room:
    id: str
    name: str
    seats: int


@dataclass
class Extra:
    code: str
    name: str
    price_cents: int


@dataclass
class Member:
    id: str
    name: str
    email: str
    nif: str
    plan: str
    joined: date
    left: Optional[date] = None
    # Código de descuento (AMIGO10, ANUAL15) o None.
    discount: Optional[str] = None

    @property
    def first_name(self) -> str:
        return self.name.split()[0]


@dataclass
class Booking:
    id: int
    member: str
    room: str
    day: date
    start: str
    end: str
    cancelled: bool = False


@dataclass
class Consumption:
    member: str
    extra: str
    qty: int
    day: date


@dataclass
class Line:
    description: str
    cents: int


@dataclass
class Invoice:
    number: str
    member_id: str
    month: str
    lines: list = field(default_factory=list)
    total_cents: int = 0
    subtotal_cents: int = 0
    vat_cents: int = 0
    paid_on: Optional[date] = None


@dataclass
class CreditNote:
    number: str
    invoice: str
    total_cents: int
    reason: str


@dataclass
class AccessEntry:
    member: str
    day: date
    time: str
    door: str
