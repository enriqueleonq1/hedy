import unittest
from datetime import datetime

from salon.booking import book
from salon.export import to_csv
from salon.reminders import reminder_message
from salon.search import by_phone


class Salon(unittest.TestCase):
    def setUp(self):
        self.appointment = book("Ana", "+58 412 555 0101", "Corte", datetime(2025, 3, 5, 15, 30))

    def test_reminder_goes_to_the_normalized_phone(self):
        self.assertEqual(reminder_message(self.appointment)["to"], "+584125550101")

    def test_search_by_phone(self):
        self.assertEqual(by_phone([self.appointment], "+58-412-555-0101"), [self.appointment])

    def test_export(self):
        self.assertIn("+584125550101", to_csv([self.appointment]))


if __name__ == "__main__":
    unittest.main()
