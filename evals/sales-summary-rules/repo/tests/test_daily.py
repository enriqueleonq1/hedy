import unittest
from decimal import Decimal

from reports.daily import daily_total


class DailyTest(unittest.TestCase):
    def test_adds_the_day(self):
        sales = [{"date": "2025-03-01", "amount": "10.10"}, {"date": "2025-03-02", "amount": "5"}]
        self.assertEqual(daily_total(sales, "2025-03-01"), Decimal("10.10"))
