import unittest
from datetime import date

from cowork import extras, rooms
from tests.helpers import fresh_store


class RoomsTest(unittest.TestCase):
    def setUp(self):
        self.store = fresh_store(self)

    def test_book_and_schedule(self):
        booking = rooms.book(self.store, "S001", "azul", date(2025, 3, 10), "10:00", "11:30")
        self.assertEqual(booking.id, 1)
        self.assertEqual(rooms.member_minutes(self.store, "S001", "2025-03"), 90)
        text = rooms.schedule_text(self.store, date(2025, 3, 10))
        self.assertIn("  10:00-11:30 S001", text)
        self.assertIn("Verde:\n  (libre)", text)

    def test_unknown_room(self):
        with self.assertRaises(ValueError):
            rooms.book(self.store, "S001", "roja", date(2025, 3, 10), "10:00", "11:00")


class ExtrasTest(unittest.TestCase):
    def test_consume(self):
        store = fresh_store(self)
        extras.consume(store, "S001", "cafe", 3, date(2025, 3, 10))
        self.assertEqual(len(extras.consumptions_in_month(store, "S001", "2025-03")), 1)
        with self.assertRaises(ValueError):
            extras.consume(store, "S001", "cafe", 0, date(2025, 3, 10))
