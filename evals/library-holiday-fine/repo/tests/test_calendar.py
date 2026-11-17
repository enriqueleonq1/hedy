import unittest
from datetime import date

from library.calendar.closures import ClosureCalendar
from library.calendar.loader import applies_to


class Closures(unittest.TestCase):
    def setUp(self):
        self.calendar = ClosureCalendar(holidays={date(2025, 1, 1)})

    def test_sundays_and_holidays_are_closed(self):
        self.assertFalse(self.calendar.is_open(date(2025, 3, 9)))  # Sunday
        self.assertFalse(self.calendar.is_open(date(2025, 1, 1)))
        self.assertTrue(self.calendar.is_open(date(2025, 3, 8)))  # Saturday

    def test_next_open_day(self):
        self.assertEqual(self.calendar.next_open_day(date(2025, 3, 7)), date(2025, 3, 7))
        self.assertEqual(self.calendar.next_open_day(date(2025, 3, 9)), date(2025, 3, 10))
        self.assertEqual(self.calendar.next_open_day(date(2025, 1, 1)), date(2025, 1, 2))

    def test_open_days_between_skips_sundays(self):
        # Fri 7 -> Tue 11: Sat 8, Mon 10, Tue 11
        self.assertEqual(self.calendar.open_days_between(date(2025, 3, 7), date(2025, 3, 11)), 3)
        self.assertEqual(self.calendar.open_days_between(date(2025, 3, 7), date(2025, 3, 7)), 0)

    def test_add_open_days(self):
        # Fri 7 + 3 open days: Sat 8, Mon 10, Tue 11
        self.assertEqual(self.calendar.add_open_days(date(2025, 3, 7), 3), date(2025, 3, 11))


class Loader(unittest.TestCase):
    def test_local_holidays_only_close_their_branches(self):
        national = {"date": "2025-05-01"}
        local = {"date": "2025-06-24", "branches": ["norte"]}
        self.assertTrue(applies_to(national, "centro"))
        self.assertTrue(applies_to(national, None))
        self.assertTrue(applies_to(local, "norte"))
        self.assertFalse(applies_to(local, "centro"))
        self.assertFalse(applies_to(local, None))


if __name__ == "__main__":
    unittest.main()
