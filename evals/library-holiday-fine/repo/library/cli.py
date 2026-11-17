"""Front desk command line: `python3 -m library <command>`."""
import argparse
import sys
from datetime import date

from library.catalog.search import available_copies, search
from library.members.tiers import tier_label
from library.money import fmt_cents
from library.reports.circulation import monthly_circulation, render_circulation
from library.reports.fines_report import monthly_fines, render_fines
from library.reports.overdue import overdue_on, render_overdue
from library.storage.csv_store import load_library


def _month(text: str) -> tuple[int, int]:
    try:
        year, month = text.split("-")
        return int(year), int(month)
    except ValueError:
        raise argparse.ArgumentTypeError("usa AAAA-MM, por ejemplo 2025-03") from None


def _day(text: str) -> date:
    try:
        return date.fromisoformat(text)
    except ValueError:
        raise argparse.ArgumentTypeError("usa AAAA-MM-DD") from None


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="library", description="Biblioteca municipal")
    parser.add_argument("--fixtures", help="carpeta con los CSV (por defecto fixtures/)")
    commands = parser.add_subparsers(dest="command", required=True)

    report = commands.add_parser("report", help="reportes mensuales")
    kinds = report.add_subparsers(dest="report", required=True)
    fines = kinds.add_parser("fines", help="multas del mes")
    fines.add_argument("month", type=_month)
    circulation = kinds.add_parser("circulation", help="préstamos del mes por sucursal")
    circulation.add_argument("month", type=_month)
    overdue = kinds.add_parser("overdue", help="préstamos vencidos a una fecha")
    overdue.add_argument("day", type=_day)

    find = commands.add_parser("search", help="buscar en el catálogo")
    find.add_argument("query")
    find.add_argument("--branch")

    member = commands.add_parser("member", help="ficha de un socio")
    member.add_argument("member_id")
    return parser


def run(args, out) -> int:
    library = load_library(args.fixtures)
    if args.command == "report":
        if args.report == "fines":
            year, month = args.month
            print(render_fines(monthly_fines(library, year, month), year, month), file=out)
        elif args.report == "circulation":
            year, month = args.month
            print(render_circulation(monthly_circulation(library, year, month), year, month), file=out)
        else:
            print(render_overdue(overdue_on(library, args.day), args.day), file=out)
    elif args.command == "search":
        lent = library.lent_item_ids()
        for title in search(library.catalog, args.query):
            free = len(available_copies(library.catalog, title.id, lent, args.branch))
            print(f"{title.id}  {title.label()}  disponibles: {free}", file=out)
    elif args.command == "member":
        m = library.members.get(args.member_id)
        if m is None:
            print(f"No existe el socio {args.member_id}", file=sys.stderr)
            return 1
        print(f"{m.name} ({tier_label(m.tier)}, {m.home_branch})", file=out)
        print(f"Préstamos activos: {len(library.active_loans(m.id))}", file=out)
        print(f"Multas pendientes: {fmt_cents(library.ledger.unpaid_cents(m.id))}", file=out)
    return 0


def main(argv=None, out=None) -> int:
    args = build_parser().parse_args(argv)
    return run(args, out or sys.stdout)
