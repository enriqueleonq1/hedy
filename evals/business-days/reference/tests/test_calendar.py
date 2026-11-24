import unittest
from datetime import date

from calendar_utils import business_days


class BusinessDays(unittest.TestCase):
    def test_empty_range(self):
        self.assertEqual(business_days(date(2025, 3, 8), date(2025, 3, 7)), 0)

    def test_starts_on_a_business_day(self):
        self.assertEqual(business_days(date(2025, 3, 3), date(2025, 3, 7)), 5)


if __name__ == "__main__":
    unittest.main()
