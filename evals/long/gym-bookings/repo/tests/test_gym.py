import io
import unittest
from datetime import date, datetime

from gym.app import Gym
from gym.catalog import format_class
from gym.cli import main
from gym.models import GymError
from gym.pricing import money, vat_of
from gym.schedule import conflicts

from .helpers import make_folder, remove

NOW = datetime(2025, 3, 10, 9, 0)


class GymTest(unittest.TestCase):
    def setUp(self):
        self.folder = make_folder()
        self.gym = Gym.load(self.folder)

    def tearDown(self):
        remove(self.folder)

    def test_format_class(self):
        self.assertEqual(format_class(self.gym.gym_class("yoga")), "Yoga · lun 10/03 18:00 · Marta (2 plazas)")

    def test_book_and_attendees(self):
        self.gym.book("s002", "yoga", NOW)
        self.gym.book("s001", "yoga", NOW)
        self.assertEqual(self.gym.attendees("yoga"), ["Luis Gómez", "Ana Pérez"])
        self.assertEqual(self.gym.outbox[0].body, "Reserva confirmada: Yoga el 10/03 a las 18:00")

    def test_unknown_member(self):
        with self.assertRaises(GymError) as raised:
            self.gym.book("s999", "yoga", NOW)
        self.assertEqual(str(raised.exception), "Socio inexistente: s999")

    def test_cancel(self):
        booking = self.gym.book("s002", "yoga", NOW)
        self.gym.cancel(booking.id, NOW)
        self.assertEqual(self.gym.attendees("yoga"), [])

    def test_invoice(self):
        text = self.gym.invoice("s002", "2025-03").text()
        self.assertEqual(
            text.splitlines(),
            ["Factura — Luis Gómez — 2025-03", "Plan basic: 29,90 €", "Subtotal: 29,90 €", "IVA (21%): 6,28 €", "Total: 36,18 €"],
        )

    def test_money_and_vat(self):
        self.assertEqual(money(123456), "1.234,56 €")
        self.assertEqual(vat_of(2990), 628)

    def test_balance(self):
        self.gym.record_payment("s001", 3000, date(2025, 3, 2))
        self.assertEqual(self.gym.balance("s001", "2025-03"), 4990 + 1048 - 3000)

    def test_state_survives_save_and_load(self):
        self.gym.book("s002", "spin", NOW)
        self.gym.add_member("s004", "Rita  Gil", "RITA@example.com ", "premium")
        self.gym.save()
        again = Gym.load(self.folder)
        self.assertEqual(again.attendees("spin"), ["Luis Gómez"])
        self.assertEqual(again.member("s004").email, "rita@example.com")
        self.assertEqual(again.member("s004").name, "Rita Gil")

    def test_occupancy(self):
        self.gym.book("s002", "spin", NOW)
        self.assertEqual(self.gym.occupancy(date(2025, 3, 11)).splitlines()[1], "07:30 Spinning (Raúl) 1/3")

    def test_cli_book(self):
        out = io.StringIO()
        code = main(["--datos", str(self.folder), "--ahora", "2025-03-10T09:00", "reservar", "s001", "yoga"], out=out)
        self.assertEqual((code, out.getvalue()), (0, "Reserva 1\n"))

    def test_schedule_conflicts(self):
        self.assertEqual(conflicts(self.gym.classes), [])


if __name__ == "__main__":
    unittest.main()
