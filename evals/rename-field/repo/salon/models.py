from dataclasses import dataclass
from datetime import datetime


@dataclass
class Appointment:
    customer_name: str
    customer_phone: str
    service: str
    starts_at: datetime
