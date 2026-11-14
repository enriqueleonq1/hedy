import unittest
from datetime import date

from calendar_utils import business_days


class HiddenBusinessDays(unittest.TestCase):
    def test_week(self):
        self.assertEqual(business_days(date(2025, 3, 3), date(2025, 3, 7)), 5)

    def test_starts_on_saturday(self):
        self.assertEqual(business_days(date(2025, 3, 8), date(2025, 3, 14)), 5)

    def test_single_day(self):
        self.assertEqual(business_days(date(2025, 3, 5), date(2025, 3, 5)), 1)
        self.assertEqual(business_days(date(2025, 3, 9), date(2025, 3, 9)), 0)

    def test_two_weeks(self):
        self.assertEqual(business_days(date(2025, 3, 3), date(2025, 3, 16)), 10)


if __name__ == "__main__":
    unittest.main()
