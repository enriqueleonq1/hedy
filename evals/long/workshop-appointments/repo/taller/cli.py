"""La línea de comandos del mostrador: python3 -m taller <comando>."""
import argparse
import sys

from . import export, history, importer, invoices, quotes, reports
from .errors import ShopError
from .shop import Shop
from .storage import DEFAULT_PATH


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="taller", description="El mostrador del taller")
    parser.add_argument("--data", default=str(DEFAULT_PATH), help="archivo de datos")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("clientes", help="clientes y sus vehículos")
    sub.add_parser("vehiculos", help="vehículos registrados")
    sub.add_parser("mecanicos", help="mecánicos y especialidades")
    sub.add_parser("servicios", help="servicios y mano de obra")
    parts = sub.add_parser("repuestos", help="repuestos y stock")
    parts.add_argument("--bajo-stock", action="store_true", help="solo los que están en el mínimo")
    quote = sub.add_parser("presupuesto", help="presupuesto de servicios para un vehículo")
    quote.add_argument("patente")
    quote.add_argument("servicios", nargs="+")
    orders = sub.add_parser("ordenes", help="órdenes de trabajo")
    orders.add_argument("--abiertas", action="store_true")
    report = sub.add_parser("reporte", help="facturación del mes")
    report.add_argument("mes", help="AAAA-MM")
    hist = sub.add_parser("historial", help="servicios hechos a un vehículo")
    hist.add_argument("patente")
    invoice = sub.add_parser("factura", help="factura de una orden cerrada")
    invoice.add_argument("orden", type=int)
    new_customer = sub.add_parser("cliente-nuevo", help="da de alta un cliente")
    new_customer.add_argument("nombre")
    new_customer.add_argument("--telefono", default="")
    new_customer.add_argument("--correo", default="")
    new_vehicle = sub.add_parser("vehiculo-nuevo", help="da de alta un vehículo de un cliente")
    new_vehicle.add_argument("patente")
    new_vehicle.add_argument("cliente")
    new_vehicle.add_argument("marca")
    new_vehicle.add_argument("modelo")
    new_vehicle.add_argument("anio", type=int)
    exp = sub.add_parser("exportar", help="órdenes cerradas del mes en CSV")
    exp.add_argument("mes", help="AAAA-MM")
    sub.add_parser("stock-csv", help="stock valorizado en CSV")
    per = sub.add_parser("por-mecanico", help="órdenes cerradas por mecánico en el mes")
    per.add_argument("mes", help="AAAA-MM")
    imp = sub.add_parser("importar", help="alta de clientes y vehículos desde un CSV")
    imp.add_argument("archivo")
    return parser


def main(argv=None, out=None) -> int:
    out = out or sys.stdout
    args = _parser().parse_args(argv)
    try:
        shop = Shop.load(args.data)
        if args.command == "clientes":
            text = reports.customers_text(shop.customers)
        elif args.command == "vehiculos":
            text = reports.vehicles_text(shop.customers)
        elif args.command == "mecanicos":
            text = reports.mechanics_text(shop.staff)
        elif args.command == "servicios":
            text = reports.services_text(shop.catalog)
        elif args.command == "repuestos":
            parts = shop.inventory.low_stock() if args.bajo_stock else shop.inventory.all()
            text = reports.parts_text(parts)
        elif args.command == "presupuesto":
            text = quotes.text(shop.quote(args.patente, [s.upper() for s in args.servicios]))
        elif args.command == "ordenes":
            text = reports.orders_text(shop.orders.open() if args.abiertas else shop.orders.all())
        elif args.command == "reporte":
            text = reports.monthly_text(shop.orders, args.mes)
        elif args.command == "historial":
            text = history.history_text(shop.customers, shop.catalog, shop.orders, args.patente)
        elif args.command == "factura":
            text = invoices.invoice_text(shop, args.orden)
        elif args.command == "cliente-nuevo":
            customer = shop.customers.add(args.nombre, args.telefono, args.correo)
            shop.save()
            text = f"Cliente {customer.id}: {customer.name}"
        elif args.command == "vehiculo-nuevo":
            vehicle = shop.customers.add_vehicle(args.patente, args.cliente, args.marca, args.modelo, args.anio)
            shop.save()
            text = f"Vehículo {vehicle.plate}: {vehicle.label()}"
        elif args.command == "exportar":
            text = export.orders_csv(shop, args.mes).rstrip("\n")
        elif args.command == "stock-csv":
            text = export.parts_csv(shop).rstrip("\n")
        elif args.command == "por-mecanico":
            text = reports.mechanics_month_text(shop.staff, shop.orders, args.mes)
        elif args.command == "importar":
            with open(args.archivo, encoding="utf-8") as f:
                added, errors = importer.import_csv(shop, f.read())
            shop.save()
            text = "\n".join([f"{added} filas importadas"] + errors)
        else:  # pragma: no cover
            raise ShopError(f"Comando desconocido: {args.command}")
    except ShopError as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1
    print(text, file=out)
    return 0
