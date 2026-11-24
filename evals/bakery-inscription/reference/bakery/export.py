"""The CSV the accountant imports every night."""
import csv
import io

from .models import Order
from .money import plain
from .pricing import order_total_cents


def _yes_no(value: bool) -> str:
    return "sí" if value else "no"


# Column -> value of an order. The accountant's spreadsheet reads columns by
# name, so new columns go at the end and existing ones keep their name.
COLUMNS = {
    "archivo": lambda o: o.source,
    "cliente": lambda o: o.customer,
    "fecha": lambda o: o.pickup_date.isoformat(),
    "hora": lambda o: o.pickup_time.strftime("%H:%M"),
    "total": lambda o: plain(order_total_cents(o)),
    "pagado": lambda o: _yes_no(o.paid),
    "nota": lambda o: o.note or "",
    "domicilio": lambda o: o.delivery_address or "",
    "caja_regalo": lambda o: _yes_no(o.gift_box),
    "dedicatoria": lambda o: o.inscription or "",
}


def to_csv(orders: list[Order]) -> str:
    out = io.StringIO()
    writer = csv.writer(out, lineterminator="\n")
    writer.writerow(COLUMNS)
    for order in orders:
        writer.writerow([value(order) for value in COLUMNS.values()])
    return out.getvalue()
