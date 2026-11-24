"""Línea de comandos del mostrador: python3 -m cowork <comando> ...

La carpeta de datos es --datos (por defecto, data/)."""

import argparse
import sys

from . import access, billing, extras, guests, lockers, members, messages, plans, reports, rooms
from .dates import parse_day
from .storage import Store


def build_parser():
    parser = argparse.ArgumentParser(prog="cowork", description="Gestión del coworking")
    parser.add_argument("--datos", default="data", help="carpeta de datos")
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("socios", help="lista los socios")
    sub.add_parser("planes", help="lista los planes")
    sub.add_parser("extras", help="lista los extras")
    p = sub.add_parser("salas", help="agenda de salas de un día")
    p.add_argument("dia")

    p = sub.add_parser("alta", help="da de alta un socio")
    p.add_argument("nombre")
    p.add_argument("email")
    p.add_argument("nif")
    p.add_argument("plan")
    p.add_argument("fecha")

    p = sub.add_parser("plan", help="cambia el plan de un socio")
    p.add_argument("socio")
    p.add_argument("plan")

    p = sub.add_parser("reservar", help="reserva una sala")
    p.add_argument("socio")
    p.add_argument("sala")
    p.add_argument("dia")
    p.add_argument("inicio")
    p.add_argument("fin")

    p = sub.add_parser("consumo", help="registra un consumo")
    p.add_argument("socio")
    p.add_argument("codigo")
    p.add_argument("cantidad", type=int)
    p.add_argument("dia")

    p = sub.add_parser("entrar", help="registra una entrada")
    p.add_argument("socio")
    p.add_argument("dia")
    p.add_argument("hora")
    p.add_argument("--puerta", default="principal")

    p = sub.add_parser("facturar", help="factura el mes de un socio, o de todos con --todos")
    p.add_argument("socio", nargs="?")
    p.add_argument("mes", nargs="?")
    p.add_argument("--codigo", help="código de descuento del socio")
    p.add_argument("--todos", metavar="MES", help="factura a todos los socios del mes")

    p = sub.add_parser("nota", help="nota de crédito sobre una factura")
    p.add_argument("factura")
    p.add_argument("centavos", type=int)
    p.add_argument("motivo")

    p = sub.add_parser("cobrar", help="marca una factura cobrada")
    p.add_argument("factura")
    p.add_argument("dia")

    p = sub.add_parser("exportar", help="facturas y notas del mes en CSV")
    p.add_argument("mes")

    p = sub.add_parser("cancelar", help="cancela una reserva")
    p.add_argument("reserva", type=int)

    p = sub.add_parser("baja", help="da de baja a un socio")
    p.add_argument("socio")
    p.add_argument("dia")

    p = sub.add_parser("resumen", help="resumen del mes")
    p.add_argument("mes")

    p = sub.add_parser("asistencia", help="asistencia del mes")
    p.add_argument("mes")

    sub.add_parser("exportar-socios", help="socios en CSV")

    p = sub.add_parser("taquilla", help="asigna una taquilla")
    p.add_argument("socio")
    p.add_argument("numero", type=int, nargs="?")
    p = sub.add_parser("liberar-taquilla", help="libera la taquilla de un socio")
    p.add_argument("socio")
    sub.add_parser("taquillas", help="taquillas ocupadas")

    p = sub.add_parser("invitar", help="registra un invitado")
    p.add_argument("socio")
    p.add_argument("nombre")
    p.add_argument("dia")
    p = sub.add_parser("invitados", help="invitados de un día")
    p.add_argument("dia")
    return parser


def run(args, store, out):
    command = args.command
    if command == "socios":
        print(members.members_text(store), file=out)
    elif command == "planes":
        print(plans.plans_text(store), file=out)
    elif command == "extras":
        print(extras.extras_text(store), file=out)
    elif command == "salas":
        print(rooms.schedule_text(store, parse_day(args.dia)), file=out)
    elif command == "alta":
        member = members.add_member(
            store, members.next_id(store), args.nombre, args.email, args.nif, args.plan,
            parse_day(args.fecha),
        )
        messages.send_welcome(store, member)
        store.save()
        print(f"Alta: {member.id} {member.name}", file=out)
    elif command == "plan":
        member = members.change_plan(store, args.socio, args.plan)
        store.save()
        print(f"{member.id} pasa al plan {store.plans[member.plan].name}", file=out)
    elif command == "reservar":
        booking = rooms.book(store, args.socio, args.sala, parse_day(args.dia), args.inicio, args.fin)
        messages.send_booking(store, booking)
        store.save()
        print(f"Reserva {booking.id}: {booking.room} {booking.day} {booking.start}-{booking.end}", file=out)
    elif command == "consumo":
        extras.consume(store, args.socio, args.codigo, args.cantidad, parse_day(args.dia))
        store.save()
        print("Consumo registrado", file=out)
    elif command == "entrar":
        access.enter(store, args.socio, parse_day(args.dia), args.hora, args.puerta)
        store.save()
        print("Entrada registrada", file=out)
    elif command == "facturar":
        if args.todos:
            emitted = billing.invoice_all(store, args.todos)
            store.save()
            print(f"{len(emitted)} facturas emitidas", file=out)
            return 0
        if not (args.socio and args.mes):
            raise ValueError("Uso: facturar SOCIO MES [--codigo CÓDIGO] o facturar --todos MES")
        if args.codigo:
            members.set_discount(store, args.socio, args.codigo)
        item = billing.invoice(store, args.socio, args.mes)
        store.save()
        print(billing.invoice_text(store, item), file=out)
    elif command == "nota":
        note = billing.credit_note(store, args.factura, args.centavos, args.motivo)
        store.save()
        print(billing.credit_note_text(note), file=out)
    elif command == "cobrar":
        item = billing.mark_paid(store, args.factura, parse_day(args.dia))
        store.save()
        print(f"{item.number} cobrada", file=out)
    elif command == "exportar":
        out.write(reports.invoices_csv(store, args.mes))
    elif command == "cancelar":
        booking = rooms.cancel_booking(store, args.reserva)
        store.save()
        print(f"Reserva {booking.id} cancelada", file=out)
    elif command == "baja":
        member = members.deactivate(store, args.socio, parse_day(args.dia))
        store.save()
        print(f"Baja: {member.id} {member.name}", file=out)
    elif command == "resumen":
        print(reports.month_report(store, args.mes), file=out)
    elif command == "asistencia":
        print(reports.attendance_text(store, args.mes), file=out)
    elif command == "exportar-socios":
        out.write(reports.members_csv(store))
    elif command == "taquilla":
        number = lockers.assign(store, args.socio, args.numero)
        store.save()
        print(f"Taquilla {number} para {args.socio}", file=out)
    elif command == "liberar-taquilla":
        number = lockers.release(store, args.socio)
        store.save()
        print(f"Taquilla {number} libre", file=out)
    elif command == "taquillas":
        print(lockers.lockers_text(store), file=out)
    elif command == "invitar":
        guests.invite(store, args.socio, args.nombre, parse_day(args.dia))
        store.save()
        print("Invitado registrado", file=out)
    elif command == "invitados":
        print(guests.guests_text(store, parse_day(args.dia)), file=out)
    return 0


def main(argv=None, out=None):
    out = out or sys.stdout
    args = build_parser().parse_args(argv)
    store = Store.load(args.datos)
    try:
        return run(args, store, out)
    except (ValueError, KeyError) as error:
        message = error.args[0] if error.args else str(error)
        print(f"Error: {message}", file=sys.stderr)
        return 1
