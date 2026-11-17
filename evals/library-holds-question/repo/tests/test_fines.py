import unittest
from datetime import date

from library.calendar.closures import ClosureCalendar
from library.catalog.models import Item
from library.fines.calculator import fine_for
from library.loans.due import due_date
from library.loans.models import Loan
from library.members.models import Member

BOOK = Item("I1", "T1", "book", "centro")
DVD = Item("I2", "T2", "dvd", "centro")
ADULT = Member("M1", "Ana", "adult", "centro")
STUDENT = Member("M2", "Beto", "student", "centro")
STAFF = Member("M3", "Caro", "staff", "centro")

# Only Sundays closed: these tests do not depend on data/holidays.json.
SUNDAYS = ClosureCalendar()


def loan(item=BOOK, member=ADULT, out=date(2025, 3, 10), renewals=0):
    return Loan("L1", item.id, member.id, "centro", out, renewals=renewals)


class DueDate(unittest.TestCase):
    def test_book_for_an_adult(self):
        # Mon 10 + 14 = Mon 24
        self.assertEqual(due_date(loan(), BOOK, ADULT, SUNDAYS), date(2025, 3, 24))

    def test_students_get_more_days(self):
        self.assertEqual(due_date(loan(member=STUDENT), BOOK, STUDENT, SUNDAYS), date(2025, 3, 31))

    def test_renewal_adds_a_period(self):
        self.assertEqual(due_date(loan(renewals=1), BOOK, ADULT, SUNDAYS), date(2025, 4, 7))

    def test_due_on_sunday_moves_to_monday(self):
        # Sun 9 + 14 = Sun 23 -> Mon 24
        self.assertEqual(due_date(loan(out=date(2025, 3, 9)), BOOK, ADULT, SUNDAYS), date(2025, 3, 24))


class Fines(unittest.TestCase):
    def test_on_time_is_free(self):
        self.assertEqual(fine_for(loan(), BOOK, ADULT, date(2025, 3, 24), SUNDAYS), 0)

    def test_per_open_day(self):
        # due Mon 24, back Thu 27: Tue, Wed, Thu
        self.assertEqual(fine_for(loan(), BOOK, ADULT, date(2025, 3, 27), SUNDAYS), 75)

    def test_sundays_do_not_count(self):
        # due Mon 24, back Mon 31: Tue..Sat (5) + Mon 31 = 6 days
        self.assertEqual(fine_for(loan(), BOOK, ADULT, date(2025, 3, 31), SUNDAYS), 150)

    def test_dvd_rate_and_cap(self):
        dvd_loan = loan(item=DVD)  # due Mon 17
        self.assertEqual(fine_for(dvd_loan, DVD, ADULT, date(2025, 3, 19), SUNDAYS), 200)
        self.assertEqual(fine_for(dvd_loan, DVD, ADULT, date(2025, 5, 30), SUNDAYS), 1000)

    def test_staff_never_pays(self):
        self.assertEqual(fine_for(loan(member=STAFF), BOOK, STAFF, date(2025, 6, 30), SUNDAYS), 0)


if __name__ == "__main__":
    unittest.main()
