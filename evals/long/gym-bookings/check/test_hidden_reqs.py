"""One test class per requirement (R01…R24) and REG for what already worked."""
from datetime import date

from hidden_gym import EARLY, MEMBERS_BEFORE, GymCase, at


class R01(GymCase):
    def test_full_class_goes_to_the_waitlist(self):
        self.gym.book("s001", "yoga", EARLY)
        self.gym.book("s002", "yoga", EARLY)
        third = self.gym.book("s003", "yoga", EARLY)
        fourth = self.gym.book("s004", "yoga", EARLY)
        self.assertEqual(third.status, "espera")
        self.assertEqual(fourth.status, "espera")
        self.assertEqual(self.gym.waitlist("yoga"), ["Eva Ruiz", "Tomás Díaz"])
        self.assertEqual(self.gym.attendees("yoga"), ["Ana Pérez", "Luis Gómez"])
        self.assertIn("Clase llena: quedas en lista de espera (posición 1)", self.bodies("s003"))
        self.assertIn("Clase llena: quedas en lista de espera (posición 2)", self.bodies("s004"))

    def test_with_a_place_the_status_is_plaza(self):
        self.assertEqual(self.gym.book("s001", "yoga", EARLY).status, "plaza")


class R02(GymCase):
    def test_a_cancelled_place_goes_to_the_first_waiting(self):
        first = self.gym.book("s001", "yoga", EARLY)
        self.gym.book("s002", "yoga", EARLY)
        third = self.gym.book("s003", "yoga", EARLY)
        fourth = self.gym.book("s004", "yoga", EARLY)
        self.gym.cancel(first.id, EARLY)
        self.assertEqual(self.gym.attendees("yoga"), ["Luis Gómez", "Eva Ruiz"])
        self.assertEqual(self.gym.waitlist("yoga"), ["Tomás Díaz"])
        self.assertEqual(self.gym.booking(third.id).status, "plaza")
        self.assertIn("Se liberó una plaza en Yoga del 10/03 a las 18:00: ya tienes tu lugar.", self.bodies("s003"))
        before = len(self.gym.outbox)
        self.gym.cancel(fourth.id, EARLY)
        self.assertEqual(self.gym.waitlist("yoga"), [])
        self.assertEqual(self.gym.attendees("yoga"), ["Luis Gómez", "Eva Ruiz"])
        promoted = [m for m in self.gym.outbox[before:] if m.body.startswith("Se liberó")]
        self.assertEqual(promoted, [])


class R03(GymCase):
    def test_cancel_up_to_two_hours_before(self):
        booking = self.gym.book("s001", "yoga", EARLY)
        self.gym.cancel(booking.id, at("2025-03-10T16:00"))
        self.assertEqual(self.gym.attendees("yoga"), [])
        late = self.gym.book("s002", "yoga", EARLY)
        self.assertEqual(
            self.error(self.gym.cancel, late.id, at("2025-03-10T16:01")),
            "Ya no se puede cancelar: faltan menos de 2 horas",
        )
        self.assertEqual(self.gym.attendees("yoga"), ["Luis Gómez"])


class R04(GymCase):
    def test_one_booking_per_class(self):
        self.gym.book("s001", "yoga", EARLY)
        self.assertEqual(self.error(self.gym.book, "s001", "yoga", EARLY), "Ya tienes una reserva para esta clase")
        self.gym.book("s002", "yoga", EARLY)
        self.gym.book("s003", "yoga", EARLY)
        self.assertEqual(self.error(self.gym.book, "s003", "yoga", EARLY), "Ya tienes una reserva para esta clase")
        self.assertEqual(self.gym.waitlist("yoga"), ["Eva Ruiz"])


class R05(GymCase):
    def test_basic_has_three_active_places(self):
        for class_id in ("pila", "box", "func"):
            self.gym.book("s002", class_id, EARLY)
        self.assertEqual(self.error(self.gym.book, "s002", "yoga2", EARLY), "Tu plan basic permite 3 reservas activas")

    def test_waiting_does_not_count_and_started_classes_neither(self):
        self.gym.book("s001", "yoga", EARLY)
        self.gym.book("s004", "yoga", EARLY)
        for class_id in ("pila", "box", "func"):
            self.gym.book("s002", class_id, EARLY)
        self.assertEqual(self.gym.book("s002", "yoga", EARLY).status, "espera")
        # Pilates and boxing have started: one active place left.
        self.assertEqual(self.gym.book("s002", "yoga2", at("2025-03-12T21:00")).status, "plaza")

    def test_premium_and_pack_have_no_limit(self):
        for class_id in ("pila", "box", "func", "yoga2", "func2"):
            self.gym.book("s004", class_id, EARLY)
            self.gym.book("s005", class_id, EARLY)


class R06(GymCase):
    def test_a_started_class_cannot_be_booked(self):
        self.assertEqual(self.error(self.gym.book, "s001", "yoga", at("2025-03-10T18:00")), "La clase ya empezó")
        self.assertEqual(self.gym.book("s001", "yoga", at("2025-03-10T17:59")).status, "plaza")


class R07(GymCase):
    def test_check_in_window_and_no_shows(self):
        ana = self.gym.book("s001", "yoga", EARLY)
        luis = self.gym.book("s002", "yoga", EARLY)
        eva = self.gym.book("s003", "yoga", EARLY)
        self.gym.check_in(ana.id, at("2025-03-10T17:30"))
        self.assertTrue(self.gym.booking(ana.id).checked_in)
        self.assertEqual(self.error(self.gym.check_in, luis.id, at("2025-03-10T17:29")), "Check-in fuera de horario")
        self.assertEqual(self.error(self.gym.check_in, luis.id, at("2025-03-10T19:01")), "Check-in fuera de horario")
        self.assertEqual(
            self.error(self.gym.check_in, eva.id, at("2025-03-10T18:00")),
            "Solo puede hacer check-in quien tiene plaza",
        )
        self.assertFalse(self.gym.booking(luis.id).checked_in)
        self.assertEqual(self.gym.no_shows("yoga", at("2025-03-10T18:59")), [])
        self.assertEqual(self.gym.no_shows("yoga", at("2025-03-10T19:00")), ["Luis Gómez"])

    def test_check_in_until_the_end(self):
        ana = self.gym.book("s001", "yoga", EARLY)
        self.gym.check_in(ana.id, at("2025-03-10T19:00"))
        self.assertEqual(self.gym.no_shows("yoga", at("2025-03-10T20:00")), [])


class R08(GymCase):
    def test_two_no_shows_block_for_a_week(self):
        self.gym.book("s002", "early", EARLY)
        self.gym.book("s002", "early2", EARLY)
        self.gym.book("s003", "early", EARLY)
        self.assertEqual(
            self.error(self.gym.book, "s002", "yoga", at("2025-03-06T09:00")),
            "Reservas bloqueadas hasta el 12/03 por inasistencias",
        )
        # One no-show alone does not block.
        self.assertEqual(self.gym.book("s003", "yoga", at("2025-03-06T09:00")).status, "plaza")
        self.assertEqual(self.gym.book("s002", "box", at("2025-03-12T08:00")).status, "plaza")

    def test_coming_is_not_a_no_show(self):
        first = self.gym.book("s002", "early", EARLY)
        self.gym.book("s002", "early2", EARLY)
        self.gym.check_in(first.id, at("2025-03-03T18:10"))
        self.assertEqual(self.gym.book("s002", "yoga", at("2025-03-06T09:00")).status, "plaza")


class R09(GymCase):
    def test_pack_pays_the_classes_it_came_to(self):
        a = self.gym.book("s005", "early", EARLY)
        b = self.gym.book("s005", "early2", EARLY)
        self.gym.book("s005", "yoga", EARLY)
        self.gym.check_in(a.id, at("2025-03-03T18:00"))
        self.gym.check_in(b.id, at("2025-03-05T18:00"))
        lines = self.lines("s005", "2025-03")
        self.assertIn(("Clases sueltas: 2 × 6,00 €", 1200), lines)
        self.assertFalse([l for l in lines if str(l[0]).startswith("Plan")], lines)
        self.assertIn(("Clases sueltas: 0 × 6,00 €", 0), self.lines("s005", "2025-04"))

    def test_pack_can_be_given(self):
        self.gym.add_member("s010", "Nuevo Socio", "nuevo@example.com", "pack")
        self.assertIn(("Clases sueltas: 0 × 6,00 €", 0), self.lines("s010", "2025-03"))


class R10(GymCase):
    def test_basic_pays_its_no_shows(self):
        self.gym.book("s002", "early", EARLY)
        self.gym.book("s002", "early2", EARLY)
        self.gym.book("s004", "early", EARLY)
        self.assertIn(("Inasistencias: 2 × 3,00 €", 600), self.lines("s002", "2025-03"))
        self.assertFalse([l for l in self.lines("s004", "2025-03") if str(l[0]).startswith("Inasistencias")])
        self.assertFalse([l for l in self.lines("s003", "2025-03") if str(l[0]).startswith("Inasistencias")])

    def test_pack_too(self):
        self.gym.book("s005", "early", EARLY)
        self.assertIn(("Inasistencias: 1 × 3,00 €", 300), self.lines("s005", "2025-03"))


class R11(GymCase):
    def test_family_discount(self):
        ana = self.lines("s001", "2025-03")
        hugo = self.lines("s006", "2025-03")
        self.assertEqual(ana[:2], [("Plan premium", 4990), ("Descuento familiar (10%)", -499)])
        self.assertEqual(hugo[:2], [("Plan basic", 2990), ("Descuento familiar (10%)", -299)])
        self.assertEqual(self.lines("s002", "2025-03"), [("Plan basic", 2990)])

    def test_one_person_is_not_a_family_and_pack_has_none(self):
        self.gym.add_member("s008", "Sola Gil", "sola@example.com", "basic", "gil")
        self.gym.add_member("s009", "Pía Pérez", "pia@example.com", "pack", "perez")
        self.assertFalse([l for l in self.lines("s008", "2025-03") if "familiar" in str(l[0])])
        self.assertFalse([l for l in self.lines("s009", "2025-03") if "familiar" in str(l[0])])
        self.assertIn(("Descuento familiar (10%)", -499), self.lines("s001", "2025-03"))


class R12(GymCase):
    def test_numbers(self):
        first = self.gym.invoice("s002", "2025-03")
        self.assertEqual(first.number, "F-2025-0001")
        self.assertEqual(first.text().splitlines()[0], "Factura F-2025-0001 — Luis Gómez — 2025-03")
        self.assertEqual(self.gym.invoice("s002", "2025-03").number, "F-2025-0001")
        self.assertEqual(self.gym.invoice("s003", "2025-03").number, "F-2025-0002")
        self.assertEqual(self.gym.invoice("s002", "2025-04").number, "F-2025-0003")
        self.assertEqual(self.gym.invoice("s004", "2026-01").number, "F-2026-0004")


class R13(GymCase):
    def test_frozen_days(self):
        self.gym.freeze("s002", date(2025, 3, 1), date(2025, 3, 9))
        self.assertEqual(self.lines("s002", "2025-03")[0], ("Plan basic (22 de 31 días)", 2122))
        self.assertEqual(self.lines("s002", "2025-04")[0], ("Plan basic", 2990))
        self.assertEqual(self.gym.book("s002", "yoga", EARLY).status, "plaza")
        self.gym.freeze("s003", date(2025, 3, 10), date(2025, 3, 20))
        self.assertEqual(self.error(self.gym.book, "s003", "yoga", EARLY), "Tu membresía está congelada hasta el 20/03")
        self.assertEqual(self.error(self.gym.book, "s003", "func3", EARLY), "Tu membresía está congelada hasta el 20/03")


class R14(GymCase):
    def test_emails(self):
        for bad in ["sin-arroba.com", "a@b", "a b@example.com", "@example.com", "a@@example.com", "a@example.", "a@.example"]:
            self.assertEqual(self.error(self.gym.add_member, "s020", "Alguien", bad, "basic"), f"Email inválido: {bad}", bad)
        self.assertEqual(
            self.error(self.gym.add_member, "s020", "Alguien", " LUIS@Example.com ", "basic"),
            "Ya existe un socio con ese email",
        )
        self.gym.add_member("s020", "Alguien", "alguien@example.com", "basic")

    def test_the_import_reports_them(self):
        from gym.importer import import_members

        path = self.folder / "nuevos.csv"
        path.write_text("id,name,email,plan\ns030,Uno,uno@example.com,basic\ns031,Dos,malo,basic\ns032,Tres,ANA@example.com,basic\n", encoding="utf-8")
        report = import_members(self.gym.state, path)
        self.assertEqual(report.added, ["s030"])
        self.assertEqual(report.skipped, [(3, "Email inválido: malo"), (4, "Ya existe un socio con ese email")])


class R15(GymCase):
    def test_from_next_month(self):
        self.gym.change_plan("s002", "premium", at("2025-03-15T10:00"))
        self.assertEqual(self.lines("s002", "2025-03")[0], ("Plan basic", 2990))
        self.assertEqual(self.lines("s002", "2025-04")[0], ("Plan premium", 4990))
        self.assertEqual(self.error(self.gym.change_plan, "s002", "gold", at("2025-03-15T10:00")), "Plan desconocido: gold")

    def test_to_pack(self):
        self.gym.change_plan("s004", "pack", at("2025-03-31T23:00"))
        self.assertIn(("Clases sueltas: 0 × 6,00 €", 0), self.lines("s004", "2025-04"))


class R16(GymCase):
    def test_occupancy_shows_the_waitlist(self):
        for member in ("s001", "s002", "s003", "s004", "s006"):
            self.gym.book(member, "spin", EARLY)
        lines = self.gym.occupancy(date(2025, 3, 11)).splitlines()
        self.assertIn("07:30 Spinning (Raúl) 3/3 · 2 en espera", lines)
        self.assertIn("19:00 Pilates (Marta) 0/5", lines)


class R17(GymCase):
    def setUp(self):
        super().setUp()
        prepare_instructors(self.gym)

    def test_report(self):
        self.assertEqual(
            self.gym.instructor_report("2025-03").strip().splitlines(),
            ["Instructor;Clases;Asistencia", "Iván;4;0%", "Marta;4;67%", "Raúl;2;—"],
        )


def prepare_instructors(gym):
    ana = gym.book("s001", "yoga", EARLY)
    gym.book("s002", "yoga", EARLY)
    gym.book("s006", "yoga", EARLY)  # waiting: does not count
    tomas = gym.book("s004", "pila", EARLY)
    gym.book("s003", "box", EARLY)
    gym.check_in(ana.id, at("2025-03-10T17:45"))
    gym.check_in(tomas.id, at("2025-03-11T19:05"))


class R18(GymCase):
    def test_class_csv(self):
        self.gym.book("s001", "yoga", EARLY)
        luis = self.gym.book("s002", "yoga", EARLY)
        self.gym.book("s003", "yoga", EARLY)
        self.gym.check_in(luis.id, at("2025-03-10T17:50"))
        self.assertEqual(
            self.rows(self.gym.class_csv("yoga")),
            [["socio", "estado", "check_in"], ["s001", "plaza", "no"], ["s002", "plaza", "sí"], ["s003", "espera", "no"]],
        )


class R19(GymCase):
    def test_reminders_once(self):
        for member in ("s001", "s002", "s005", "s006"):
            self.gym.book(member, "spin", EARLY)
        self.gym.book("s003", "pila", EARLY)
        self.gym.book("s004", "yoga", EARLY)
        self.assertEqual(self.gym.send_reminders(at("2025-03-10T09:00")), 4)
        self.assertIn("Recordatorio: Spinning mañana a las 07:30", self.bodies("s001"))
        self.assertIn("Recordatorio: Pilates mañana a las 19:00", self.bodies("s003"))
        self.assertFalse([b for b in self.bodies("s006") if b.startswith("Recordatorio")])
        self.assertFalse([b for b in self.bodies("s004") if b.startswith("Recordatorio")])
        size = len(self.gym.outbox)
        self.assertEqual(self.gym.send_reminders(at("2025-03-10T20:00")), 0)
        self.assertEqual(len(self.gym.outbox), size)


class R20(GymCase):
    def test_messages_command(self):
        self.gym.book("s001", "yoga", EARLY)
        self.gym.book("s002", "spin", EARLY)
        self.gym.save()
        code, out = self.cli("mensajes")
        self.assertEqual(code, 0)
        self.assertEqual(
            out.strip().splitlines(),
            ["s001: Reserva confirmada: Yoga el 10/03 a las 18:00", "s002: Reserva confirmada: Spinning el 11/03 a las 07:30"],
        )
        self.assertEqual(self.cli("mensajes")[1].strip(), "No hay mensajes pendientes")


class R21(GymCase):
    def test_everything_new_survives_a_reload(self):
        ana = self.gym.book("s001", "yoga", EARLY)
        self.gym.book("s002", "yoga", EARLY)
        self.gym.book("s003", "yoga", EARLY)
        self.gym.check_in(ana.id, at("2025-03-10T17:45"))
        self.assertEqual(self.gym.send_reminders(at("2025-03-09T10:00")), 2)
        self.gym.freeze("s004", date(2025, 3, 12), date(2025, 3, 14))
        self.gym.change_plan("s006", "premium", at("2025-03-05T10:00"))
        self.assertEqual(self.gym.invoice("s002", "2025-03").number, "F-2025-0001")
        gym = self.reload()
        self.assertEqual(gym.waitlist("yoga"), ["Eva Ruiz"])
        self.assertTrue(gym.booking(ana.id).checked_in)
        self.assertEqual(self.error(gym.book, "s004", "box", EARLY), "Tu membresía está congelada hasta el 14/03")
        self.assertEqual(gym.invoice("s002", "2025-03").number, "F-2025-0001")
        self.assertEqual(gym.invoice("s003", "2025-03").number, "F-2025-0002")
        self.assertEqual(self.lines("s006", "2025-04")[0], ("Plan premium", 4990))
        self.assertEqual(gym.send_reminders(at("2025-03-09T10:00")), 0)


class R22(GymCase):
    def test_check_in_no_shows_and_waitlist_commands(self):
        ana = self.gym.book("s001", "yoga", EARLY)
        self.gym.book("s002", "yoga", EARLY)
        self.gym.book("s003", "yoga", EARLY)
        self.gym.save()
        self.assertEqual(self.cli("--ahora", "2025-03-10T17:45", "checkin", str(ana.id)), (0, "Check-in registrado\n"))
        self.assertEqual(self.cli("--ahora", "2025-03-10T19:30", "faltas", "yoga"), (0, "Luis Gómez\n"))
        self.assertEqual(self.cli("espera", "yoga"), (0, "Eva Ruiz\n"))
        self.assertEqual(self.cli("--ahora", "2025-03-10T17:45", "checkin", "99"), (1, "Error: Reserva inexistente: 99\n"))


class R23(GymCase):
    def test_freeze_and_plan_commands(self):
        self.assertEqual(
            self.cli("congelar", "s002", "2025-03-15", "2025-03-20"), (0, "Membresía congelada hasta el 20/03\n")
        )
        self.assertEqual(
            self.cli("--ahora", "2025-03-15T10:00", "plan", "s002", "premium"),
            (0, "Plan de s002 cambia a premium desde el 01/04\n"),
        )
        gym = self.Gym.load(self.folder)
        self.gym = gym
        self.assertEqual(self.error(gym.book, "s002", "func3", EARLY), "Tu membresía está congelada hasta el 20/03")
        self.assertEqual(self.lines("s002", "2025-04")[0], ("Plan premium", 4990))


class R24(GymCase):
    def test_instructors_report_command(self):
        prepare_instructors(self.gym)
        self.gym.save()
        code, out = self.cli("reporte", "instructores", "2025-03")
        self.assertEqual(code, 0)
        self.assertEqual(out.strip().splitlines(), ["Instructor;Clases;Asistencia", "Iván;4;0%", "Marta;4;67%", "Raúl;2;—"])
        self.assertEqual(self.cli("reporte", "instructores", "2025-13"), (1, "Error: Mes inválido: 2025-13\n"))


class REG(GymCase):
    """What the project did before the request, still working."""

    members = MEMBERS_BEFORE

    def test_classes_and_members(self):
        from gym.catalog import format_class

        self.assertEqual(format_class(self.gym.gym_class("yoga")), "Yoga · lun 10/03 18:00 · Marta (2 plazas)")
        self.assertEqual(self.gym.member("s003").name, "Eva Ruiz")
        self.assertEqual(self.error(self.gym.book, "s999", "yoga", EARLY), "Socio inexistente: s999")
        self.assertEqual(self.error(self.gym.book, "s001", "nada", EARLY), "Clase inexistente: nada")

    def test_booking_cancel_and_messages(self):
        booking = self.gym.book("s002", "spin", EARLY)
        self.assertEqual(self.gym.attendees("spin"), ["Luis Gómez"])
        self.assertIn("Reserva confirmada: Spinning el 11/03 a las 07:30", self.bodies("s002"))
        self.gym.cancel(booking.id, EARLY)
        self.assertEqual(self.gym.attendees("spin"), [])
        self.assertIn("Reserva cancelada: Spinning el 11/03 a las 07:30", self.bodies("s002"))

    def test_invoice_balance_and_revenue(self):
        invoice = self.gym.invoice("s004", "2025-03")
        self.assertEqual([tuple(l) for l in invoice.lines], [("Plan premium", 4990)])
        self.assertEqual((invoice.subtotal, invoice.vat, invoice.total), (4990, 1048, 6038))
        self.gym.record_payment("s004", 6038, date(2025, 3, 3))
        self.assertEqual(self.gym.balance("s004", "2025-03"), 0)
        self.assertIn("Cobrado: 60,38 €", self.gym.revenue("2025-03"))

    def test_occupancy_and_schedule(self):
        from gym.schedule import week_text

        self.gym.book("s001", "spin", EARLY)
        lines = self.gym.occupancy(date(2025, 3, 11)).splitlines()
        self.assertEqual(lines[:2], ["Ocupación del 11/03/2025", "07:30 Spinning (Raúl) 1/3"])
        self.assertIn("  18:00–19:00 Yoga · Marta · Sala 2", week_text(self.gym.classes, date(2025, 3, 12)))

    def test_cli(self):
        code, out = self.cli("--ahora", "2025-03-01T09:00", "reservar", "s001", "yoga")
        self.assertEqual((code, out), (0, "Reserva 1\n"))
        self.assertEqual(self.cli("asistentes", "yoga"), (0, "Ana Pérez\n"))
        code, out = self.cli("clases")
        self.assertEqual(out.splitlines()[0], "Yoga · lun 03/03 18:00 · Marta (5 plazas)")
        self.assertEqual(self.cli("exportar", "socios")[1].splitlines()[0], "id,nombre,email,plan,familia")

    def test_save_and_load(self):
        self.gym.book("s002", "spin", EARLY)
        self.gym.add_member("s040", "Rita  Gil", "RITA2@example.com ", "premium")
        gym = self.reload()
        self.assertEqual(gym.attendees("spin"), ["Luis Gómez"])
        self.assertEqual((gym.member("s040").name, gym.member("s040").email), ("Rita Gil", "rita2@example.com"))
