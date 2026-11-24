"""Exportaciones en CSV para la contadora."""
import csv
import io

from . import money


def _amount(cents: int) -> str:
    """Importe con punto decimal y dos decimales, sin símbolo: 86515.00."""
    sign = "-" if cents < 0 else ""
    whole, rest = divmod(abs(cents), 100)
    return f"{sign}{whole}.{rest:02d}"


def orders_csv(shop, month: str) -> str:
    """Las órdenes cerradas del mes: orden,fecha,patente,cliente,servicios,total."""
    buf = io.StringIO()
    writer = csv.writer(buf, lineterminator="\n")
    writer.writerow(["orden", "fecha", "patente", "cliente", "servicios", "total"])
    for order in shop.orders.closed_in(month):
        customer = shop.customers.owner(order.plate)
        writer.writerow([order.id, order.closed, order.plate, customer.name,
                         "+".join(order.services), _amount(order.total_cents)])
    return buf.getvalue()


def parts_csv(shop) -> str:
    """El stock valorizado: codigo,repuesto,stock,precio,valor."""
    buf = io.StringIO()
    writer = csv.writer(buf, lineterminator="\n")
    writer.writerow(["codigo", "repuesto", "stock", "precio", "valor"])
    for part in shop.inventory.all():
        writer.writerow([part.code, part.name, part.stock, _amount(part.price_cents),
                         _amount(part.stock * part.price_cents)])
    return buf.getvalue()


def summary_line(shop) -> str:
    return f"Stock valorizado: {money.fmt(shop.inventory.value_cents())}"


APPOINTMENT_COLUMNS = ["id", "patente", "servicio", "mecanico", "inicio", "fin", "estado", "cliente"]


def appointments_csv(shop, day) -> str:
    """Los turnos del día, cancelados incluidos, por hora."""
    buf = io.StringIO()
    writer = csv.writer(buf, lineterminator="\n")
    writer.writerow(APPOINTMENT_COLUMNS)
    for a in shop.agenda.on(day):
        writer.writerow([a.id, a.plate, a.service, shop.staff.get(a.mechanic).name, a.start, a.end, a.status,
                         shop.customers.owner(a.plate).name])
    return buf.getvalue()
