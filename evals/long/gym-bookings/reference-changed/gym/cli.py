"""`python3 -m gym [--datos CARPETA] [--ahora AAAA-MM-DDTHH:MM] <comando> …`

Comandos:
  clases                      las clases, por hora
  socios                      los socios y su plan
  alta <id> <nombre> <email> <plan> [familia]
  reservar <socio> <clase>
  cancelar <reserva>
  asistentes <clase>
  factura <socio> <AAAA-MM>
  pago <socio> <centavos> [nota]
  reporte ocupacion <AAAA-MM-DD>
  reporte ingresos <AAAA-MM>
  horario <AAAA-MM-DD>        el horario de esa semana
  revisar                     datos inválidos y choques de horario
  importar <archivo.csv>      alta de muchos socios
  exportar socios|clases|pagos [AAAA-MM]
  espera <clase>              la lista de espera
  checkin <reserva>
  faltas <clase>              quienes no vinieron
  congelar <socio> <desde> <hasta>
  plan <socio> <plan>         desde el mes que viene
  reporte instructores <AAAA-MM>
  mensajes                    los mensajes pendientes (y los marca enviados)
"""
import argparse
import sys
from datetime import date, datetime

from .app import Gym
from .exports import classes_csv, members_csv, payments_csv
from .importer import import_members
from .catalog import by_start, format_class
from .members import format_member
from .models import GymError
from .schedule import review, week_text


def parser():
    p = argparse.ArgumentParser(prog="python3 -m gym", add_help=True)
    p.add_argument("--datos", default="data", help="carpeta de datos")
    p.add_argument("--ahora", help="fecha y hora a usar (por defecto, ahora)")
    sub = p.add_subparsers(dest="command", required=True)
    sub.add_parser("clases")
    sub.add_parser("socios")
    alta = sub.add_parser("alta")
    for name in ("id", "nombre", "email", "plan"):
        alta.add_argument(name)
    alta.add_argument("familia", nargs="?", default="")
    reservar = sub.add_parser("reservar")
    reservar.add_argument("socio")
    reservar.add_argument("clase")
    cancelar = sub.add_parser("cancelar")
    cancelar.add_argument("reserva", type=int)
    asistentes = sub.add_parser("asistentes")
    asistentes.add_argument("clase")
    factura = sub.add_parser("factura")
    factura.add_argument("socio")
    factura.add_argument("mes")
    pago = sub.add_parser("pago")
    pago.add_argument("socio")
    pago.add_argument("centavos", type=int)
    pago.add_argument("nota", nargs="?", default="")
    reporte = sub.add_parser("reporte")
    reporte.add_argument("tipo", choices=["ocupacion", "ingresos", "instructores"])
    reporte.add_argument("cuando")
    horario = sub.add_parser("horario")
    horario.add_argument("dia")
    sub.add_parser("revisar")
    importar = sub.add_parser("importar")
    importar.add_argument("archivo")
    exportar = sub.add_parser("exportar")
    exportar.add_argument("que", choices=["socios", "clases", "pagos"])
    exportar.add_argument("mes", nargs="?")
    espera = sub.add_parser("espera")
    espera.add_argument("clase")
    checkin = sub.add_parser("checkin")
    checkin.add_argument("reserva", type=int)
    faltas = sub.add_parser("faltas")
    faltas.add_argument("clase")
    congelar = sub.add_parser("congelar")
    congelar.add_argument("socio")
    congelar.add_argument("desde")
    congelar.add_argument("hasta")
    plan = sub.add_parser("plan")
    plan.add_argument("socio")
    plan.add_argument("plan")
    sub.add_parser("mensajes")
    return p


def run(gym, args, now, out):
    if args.command == "clases":
        for gym_class in by_start(gym.classes):
            print(format_class(gym_class), file=out)
    elif args.command == "socios":
        for member in sorted(gym.state.members.values(), key=lambda m: m.id):
            print(f"{member.id} {format_member(member)}", file=out)
    elif args.command == "alta":
        member = gym.add_member(args.id, args.nombre, args.email, args.plan, args.familia, now)
        gym.save()
        print(f"Socio {member.id} dado de alta", file=out)
    elif args.command == "reservar":
        booking = gym.book(args.socio, args.clase, now)
        gym.save()
        print(f"Reserva {booking.id}", file=out)
    elif args.command == "cancelar":
        gym.cancel(args.reserva, now)
        gym.save()
        print(f"Reserva {args.reserva} cancelada", file=out)
    elif args.command == "asistentes":
        for name in gym.attendees(args.clase):
            print(name, file=out)
    elif args.command == "factura":
        print(gym.invoice(args.socio, args.mes).text(), file=out)
        gym.save()
    elif args.command == "pago":
        gym.record_payment(args.socio, args.centavos, now.date(), args.nota)
        gym.save()
        print("Pago registrado", file=out)
    elif args.command == "reporte":
        if args.tipo == "ocupacion":
            print(gym.occupancy(date.fromisoformat(args.cuando)), file=out)
        elif args.tipo == "instructores":
            print(gym.instructor_report(args.cuando), file=out)
        else:
            print(gym.revenue(args.cuando), file=out)
    elif args.command == "horario":
        print(week_text(gym.classes, date.fromisoformat(args.dia)), file=out)
    elif args.command == "revisar":
        problems = review(gym.classes)
        for problem in problems:
            print(problem, file=out)
        if not problems:
            print("Todo en orden", file=out)
        return 1 if problems else 0
    elif args.command == "importar":
        report = import_members(gym.state, args.archivo)
        gym.save()
        print(report.text(), file=out)
    elif args.command == "exportar":
        if args.que == "socios":
            out.write(members_csv(gym.state))
        elif args.que == "clases":
            out.write(classes_csv(gym.state))
        else:
            out.write(payments_csv(gym.state, args.mes))
    elif args.command == "espera":
        for name in gym.waitlist(args.clase):
            print(name, file=out)
    elif args.command == "checkin":
        gym.check_in(args.reserva, now)
        gym.save()
        print("Check-in registrado", file=out)
    elif args.command == "faltas":
        for name in gym.no_shows(args.clase, now):
            print(name, file=out)
    elif args.command == "congelar":
        period = gym.freeze(args.socio, date.fromisoformat(args.desde), date.fromisoformat(args.hasta))
        gym.save()
        print(f"Membresía congelada hasta el {period.end:%d/%m}", file=out)
    elif args.command == "plan":
        change = gym.change_plan(args.socio, args.plan, now)
        gym.save()
        print(f"Plan de {args.socio} cambia a {change.plan} desde el {change.since:%d/%m}", file=out)
    elif args.command == "mensajes":
        lines = gym.take_pending_lines()
        gym.save()
        for line in lines:
            print(line, file=out)
        if not lines:
            print("No hay mensajes pendientes", file=out)
    return 0


def main(argv=None, out=None):
    out = out or sys.stdout
    args = parser().parse_args(sys.argv[1:] if argv is None else argv)
    now = datetime.fromisoformat(args.ahora) if args.ahora else datetime.now()
    gym = Gym.load(args.datos)
    try:
        return run(gym, args, now, out)
    except GymError as error:
        print(f"Error: {error}", file=out)
        return 1
