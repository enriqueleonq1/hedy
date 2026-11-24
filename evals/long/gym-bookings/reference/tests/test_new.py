import io
import unittest
from datetime import date, datetime

from gym.app import Gym
from gym.cli import main
from gym.models import GymError

from .helpers import make_folder, remove

BEFORE = datetime(2025, 3, 10, 9, 0)


class WaitlistTest(unittest.TestCase):
    def setUp(self):
        self.folder = make_folder()
        self.gym = Gym.load(self.folder)

    def tearDown(self):
        remove(self.folder)

    def test_full_class_goes_to_waitlist_and_promotes(self):
        first = self.gym.book("s001", "yoga", BEFORE)
        self.gym.book("s002", "yoga", BEFORE)
        waiting = self.gym.book("s003", "yoga", BEFORE)
        self.assertEqual(waiting.status, "espera")
        self.assertEqual(self.gym.outbox[-1].body, "Clase llena: quedas en lista de espera (posición 1)")
        self.gym.cancel(first.id, BEFORE)
        self.assertEqual(self.gym.attendees("yoga"), ["Luis Gómez", "Eva Ruiz"])
        self.assertEqual(self.gym.waitlist("yoga"), [])

    def test_late_cancel(self):
        booking = self.gym.book("s001", "yoga", BEFORE)
        with self.assertRaises(GymError):
            self.gym.cancel(booking.id, datetime(2025, 3, 10, 17, 0))

    def test_duplicate(self):
        self.gym.book("s001", "yoga", BEFORE)
        with self.assertRaises(GymError):
            self.gym.book("s001", "yoga", BEFORE)

    def test_check_in_and_no_shows(self):
        a = self.gym.book("s001", "yoga", BEFORE)
        self.gym.book("s002", "yoga", BEFORE)
        self.gym.check_in(a.id, datetime(2025, 3, 10, 17, 45))
        self.assertEqual(self.gym.no_shows("yoga", datetime(2025, 3, 10, 19, 0)), ["Luis Gómez"])

    def test_invoice_numbers_repeat(self):
        first = self.gym.invoice("s002", "2025-03").number
        self.assertEqual(self.gym.invoice("s002", "2025-03").number, first)
        self.assertEqual(self.gym.invoice("s003", "2025-03").number, "F-2025-0002")

    def test_freeze_prorates(self):
        self.gym.freeze("s002", date(2025, 3, 1), date(2025, 3, 9))
        self.assertEqual(self.gym.invoice("s002", "2025-03").lines[0], ("Plan basic (22 de 31 días)", 2122))

    def test_messages_command(self):
        self.gym.book("s001", "spin", BEFORE)
        self.gym.save()
        out = io.StringIO()
        main(["--datos", str(self.folder), "mensajes"], out=out)
        self.assertEqual(out.getvalue(), "s001: Reserva confirmada: Spinning el 11/03 a las 07:30\n")


if __name__ == "__main__":
    unittest.main()
