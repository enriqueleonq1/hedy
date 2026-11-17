"""Counter command line: `python3 -m bakery <command>`."""
import argparse
import sys

from . import filters
from .export import to_csv
from .money import fmt
from .orderfile import OrderFileError
from .pricing import day_total_cents, order_total_cents
from .storage import load_order, load_orders
from .ticket import render_ticket
from .validation import problems


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="bakery", description="Pedidos de la panadería")
    commands = parser.add_subparsers(dest="command", required=True)

    listing = commands.add_parser("list", help="pedidos de una carpeta")
    listing.add_argument("folder")
    for name, (help_text, _) in filters.FILTERS.items():
        listing.add_argument(f"--solo-{name}", dest=f"only_{name}", action="store_true", help=help_text)

    ticket = commands.add_parser("ticket", help="ticket de un pedido")
    ticket.add_argument("file")

    export = commands.add_parser("csv", help="CSV para contabilidad")
    export.add_argument("folder")

    check = commands.add_parser("check", help="revisa los pedidos de una carpeta")
    check.add_argument("folder")
    return parser


def _list(args) -> int:
    chosen = [name for name in filters.FILTERS if getattr(args, f"only_{name}")]
    orders = filters.apply(load_orders(args.folder), chosen)
    for o in orders:
        when = f"{o.pickup_date.isoformat()} {o.pickup_time.strftime('%H:%M')}"
        print(f"{o.source:<24} {when}  {o.customer:<20} {fmt(order_total_cents(o)):>9}")
    print(f"{len(orders)} pedidos, total {fmt(day_total_cents(orders))}")
    return 0


def _check(args) -> int:
    failed = 0
    for o in load_orders(args.folder):
        for problem in problems(o):
            print(f"{o.source}: {problem}")
            failed += 1
    if not failed:
        print("Todo en orden.")
    return 1 if failed else 0


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.command == "list":
            return _list(args)
        if args.command == "ticket":
            print(render_ticket(load_order(args.file)))
            return 0
        if args.command == "csv":
            sys.stdout.write(to_csv(load_orders(args.folder)))
            return 0
        return _check(args)
    except OrderFileError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 2
