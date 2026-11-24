import io
import unittest
from datetime import date

from cowork import cli, guests, lockers
from cowork.storage import Store
from tests.helpers import fresh_store


class LockersTest(unittest.TestCase):
    def test_assign_and_release(self):
        store = fresh_store(self)
        self.assertEqual(lockers.assign(store, "S001"), 1)
        self.assertEqual(lockers.assign(store, "S002", 7), 7)
        with self.assertRaises(ValueError):
            lockers.assign(store, "S003", 7)
        self.assertEqual(lockers.release(store, "S001"), 1)
        store.save()
        self.assertEqual(lockers.locker_of(Store.load(store.folder), "S002"), 7)


class GuestsTest(unittest.TestCase):
    def test_two_per_day(self):
        store = fresh_store(self)
        day = date(2025, 3, 10)
        guests.invite(store, "S001", "Marta", day)
        guests.invite(store, "S001", "Luis", day)
        with self.assertRaises(ValueError):
            guests.invite(store, "S001", "Pablo", day)
        self.assertEqual(guests.busiest_month_day(store, "2025-03"), "2025-03-10")

    def test_cli(self):
        store = fresh_store(self)
        out = io.StringIO()
        cli.main(["--datos", str(store.folder), "invitar", "S001", "Marta", "2025-03-10"], out=out)
        cli.main(["--datos", str(store.folder), "invitados", "2025-03-10"], out=out)
        self.assertIn("Marta (invita S001)", out.getvalue())
