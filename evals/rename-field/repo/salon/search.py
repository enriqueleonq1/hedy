from .models import Appointment
from .phones import normalize


def by_phone(appointments: list[Appointment], customer_phone: str) -> list[Appointment]:
    wanted = normalize(customer_phone)
    return [a for a in appointments if a.customer_phone == wanted]
