"""Facturas de las órdenes cerradas."""
from . import money
from .errors import ShopError


def number(order) -> str:
    """El número de factura de una orden: T-0001 para la orden 1."""
    return f"T-{order.id:04d}"


def invoice_text(shop, order_id: int) -> str:
    order = shop.orders.get(order_id)
    if order.status != "cerrada":
        raise ShopError(f"La orden {order.id} no está cerrada")
    vehicle = shop.customers.vehicle(order.plate)
    customer = shop.customers.get(vehicle.customer)
    quote = shop.order_quote(order)
    width = max([len(l.description) for l in quote.lines] + [10])
    lines = [
        f"Factura {number(order)} — {order.closed}",
        f"Cliente: {customer.name}",
        f"Vehículo: {vehicle.plate} {vehicle.label()}",
        "",
    ]
    for line in quote.lines:
        lines.append(f"{line.description:<{width}}  {money.fmt(line.cents):>16}")
    lines.append(f"{'Subtotal':<{width}}  {money.fmt(quote.subtotal_cents):>16}")
    lines.append(f"{'IVA 21%':<{width}}  {money.fmt(quote.vat_cents):>16}")
    lines.append(f"{'Total':<{width}}  {money.fmt(order.total_cents):>16}")
    return "\n".join(lines)
