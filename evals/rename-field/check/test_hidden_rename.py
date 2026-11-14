import unittest
from datetime import datetime

from salon.booking import book
from salon.export import COLUMNS, to_csv
from salon.models import Appointment


class Rename(unittest.TestCase):
    def test_field_renamed(self):
        a = book("Ana", "+58 412 555 0101", "Corte", datetime(2025, 3, 5, 15, 30))
        self.assertIsInstance(a, Appointment)
        self.assertEqual(a.phone_number, "+584125550101")
        self.assertIn("phone_number", COLUMNS)
        self.assertTrue(to_csv([a]).startswith("customer_name,phone_number,"))


if __name__ == "__main__":
    unittest.main()
