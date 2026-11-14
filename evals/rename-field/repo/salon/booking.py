from datetime import datetime

from .models import Appointment
from .phones import normalize


def book(customer_name: str, customer_phone: str, service: str, starts_at: datetime) -> Appointment:
    return Appointment(
        customer_name=customer_name,
        customer_phone=normalize(customer_phone),
        service=service,
        starts_at=starts_at,
    )
