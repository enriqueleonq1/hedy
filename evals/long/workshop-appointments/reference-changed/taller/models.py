"""Los datos del taller. Los importes van en centavos enteros."""
from dataclasses import dataclass, field
from datetime import date


@dataclass
class Customer:
    id: str
    name: str
    phone: str = ""
    email: str = ""


@dataclass
class Vehicle:
    plate: str
    customer: str
    brand: str
    model: str
    year: int

    def label(self) -> str:
        return f"{self.brand} {self.model} {self.year}"


@dataclass
class Mechanic:
    id: str
    name: str
    skills: list[str] = field(default_factory=list)

    def can(self, skill: str) -> bool:
        return skill in self.skills


@dataclass
class PartUse:
    code: str
    qty: int


@dataclass
class Service:
    code: str
    name: str
    minutes: int
    skill: str
    labor_cents: int
    parts: list[PartUse] = field(default_factory=list)


@dataclass
class Part:
    code: str
    name: str
    price_cents: int
    stock: int
    min_stock: int = 0


@dataclass
class WorkOrder:
    id: int
    plate: str
    mechanic: str | None
    services: list[str]
    parts: list[PartUse]
    opened: str
    closed: str | None = None
    status: str = "abierta"
    total_cents: int = 0

    @property
    def is_open(self) -> bool:
        return self.status == "abierta"


@dataclass
class Appointment:
    """Un turno: un servicio para un vehículo, con un mecánico, un día y una franja."""
    id: int
    plate: str
    service: str
    mechanic: str
    day: date
    start: str
    end: str
    status: str = "reservado"
    order: int | None = None

    @property
    def active(self) -> bool:
        """Ocupa su franja y sus repuestos (no está cancelado)."""
        return self.status != "cancelado"


@dataclass
class QuoteLine:
    description: str
    cents: int


@dataclass
class Quote:
    plate: str
    lines: list[QuoteLine]
    subtotal_cents: int
    vat_cents: int
    total_cents: int
    title: str = ""


@dataclass
class Message:
    """Un mensaje para un cliente: a quién (teléfono o correo) y qué dice."""
    to: str
    text: str
