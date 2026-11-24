import io
import unittest
from datetime import date

from cowork import billing, cli, members, money, reports, rooms
from tests.helpers import fresh_store


class MoneyTest(unittest.TestCase):
    def test_euros(self):
        self.assertEqual(money.eur(123456), "1.234,56 €")
        self.assertEqual(money.eur(-2500), "-25,00 €")
        self.assertEqual(money.plain(23940), "239.40")


class RoomsRulesTest(unittest.TestCase):
    def setUp(self):
        self.store = fresh_store(self)
        self.day = date(2025, 3, 11)

    def test_opening_hours(self):
        for start, end in (("07:30", "09:00"), ("19:30", "20:30"), ("10:15", "11:00")):
            with self.assertRaisesRegex(ValueError, "de 08:00 a 20:00 en franjas de 30 minutos"):
                rooms.book(self.store, "S001", "azul", self.day, start, end)

    def test_overlap_and_cancel(self):
        booking = rooms.book(self.store, "S001", "azul", self.day, "10:00", "11:00")
        with self.assertRaisesRegex(ValueError, "La sala Azul ya está reservada de 10:00 a 11:00"):
            rooms.book(self.store, "S002", "azul", self.day, "10:30", "11:30")
        rooms.book(self.store, "S002", "azul", self.day, "11:00", "12:00")
        rooms.cancel_booking(self.store, booking.id)
        rooms.book(self.store, "S002", "azul", self.day, "10:00", "11:00")
        with self.assertRaisesRegex(ValueError, f"Reserva inexistente: {booking.id}"):
            rooms.cancel_booking(self.store, booking.id)


class MembersRulesTest(unittest.TestCase):
    def setUp(self):
        self.store = fresh_store(self)

    def test_email_and_nif(self):
        add = lambda email, nif: members.add_member(
            self.store, "S005", "Elena Mora", email, nif, "flex", date(2025, 3, 1)
        )
        with self.assertRaisesRegex(ValueError, "Email inválido: anaexample.com"):
            add("anaexample.com", "33333333P")
        with self.assertRaisesRegex(ValueError, "NIF inválido: 12345678A"):
            add("elena@example.com", "12345678A")

    def test_discounts(self):
        with self.assertRaisesRegex(ValueError, "Código de descuento desconocido: GRATIS"):
            members.set_discount(self.store, "S001", "GRATIS")

    def test_deactivate(self):
        members.deactivate(self.store, "S001", date(2025, 3, 20))
        self.assertIn("S001 Ana Ruiz (Fijo) — baja 20/03/2025", members.members_text(self.store))


class ReportsAndCliTest(unittest.TestCase):
    def setUp(self):
        self.store = fresh_store(self)

    def test_report_and_csv(self):
        billing.invoice(self.store, "S001", "2025-03")
        rooms.book(self.store, "S001", "azul", date(2025, 3, 10), "10:00", "12:30")
        text = reports.month_report(self.store, "2025-03")
        self.assertIn("Facturado: 302,50 €", text)
        self.assertIn("Ocupación Azul: 2,5 h", text)
        self.assertEqual(
            reports.invoices_csv(self.store, "2025-03").splitlines()[1],
            "F-2025-0001,,S001,250.00,52.50,302.50",
        )

    def test_cli(self):
        out = io.StringIO()
        folder = str(self.store.folder)
        self.assertEqual(cli.main(["--datos", folder, "facturar", "--todos", "2025-03"], out=out), 0)
        self.assertEqual(cli.main(["--datos", folder, "cobrar", "F-2025-0001", "2025-04-05"], out=out), 0)
        self.assertEqual(cli.main(["--datos", folder, "nota", "F-2025-0002", "1000", "Ajuste"], out=out), 0)
        text = out.getvalue()
        self.assertIn("3 facturas emitidas", text)
        self.assertIn("F-2025-0001 cobrada", text)
        self.assertIn("Nota de crédito NC-2025-0001", text)
