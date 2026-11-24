"""Los datos del taller. Los importes van en centavos enteros."""
from dataclasses import dataclass, field


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
