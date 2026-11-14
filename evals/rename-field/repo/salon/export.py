import csv
import io

from .models import Appointment

COLUMNS = ["customer_name", "customer_phone", "service", "starts_at"]


def to_csv(appointments: list[Appointment]) -> str:
    out = io.StringIO()
    writer = csv.writer(out)
    writer.writerow(COLUMNS)
    for a in appointments:
        writer.writerow([a.customer_name, a.customer_phone, a.service, a.starts_at.isoformat()])
    return out.getvalue()
