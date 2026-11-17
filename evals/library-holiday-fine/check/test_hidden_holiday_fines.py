import io
import unittest
from datetime import date

from library.calendar.loader import load_calendar
from library.catalog.models import Item
from library.cli import main
from library.fines.calculator import fine_for
from library.loans.due import due_date
from library.loans.models import Loan
from library.members.models import Member
from library.reports.fines_report import monthly_fines
from library.storage.csv_store import load_library


def fines(year, month):
    return {(r.member, r.title): r.cents for r in monthly_fines(load_library(), year, month)}


class HolidayFines(unittest.TestCase):
    def test_laura_returned_on_the_first_open_day(self):
        out = io.StringIO()
        main(["report", "fines", "2025-03"], out=out)
        self.assertNotIn("Laura Gómez", out.getvalue())

    def test_march_report(self):
        march = fines(2025, 3)
        self.assertNotIn(("Laura Gómez", "Rayuela"), march)
        # due Sat 22, Mon 24 is a holiday: Tue 25 and Wed 26 count
        self.assertEqual(march[("Valentina Ríos", "La tregua")], 50)
        # untouched by holidays
        self.assertEqual(march[("Diego Fernández", "Ficciones")], 100)
        self.assertEqual(march[("Martín Acosta", "Metrópolis")], 300)

    def test_holidays_inside_the_late_period_do_not_count(self):
        # Easter: due Wed 16 April, back Tue 22; Thu 17 and Fri 18 closed
        self.assertEqual(fines(2025, 4)[("Elena Castro", "La casa de los espíritus")], 75)
        # due Tue 29 April, back Fri 2 May; Thu 1 May closed
        self.assertEqual(fines(2025, 5)[("Laura Gómez", "Don Segundo Sombra")], 50)

    def test_local_holidays_only_for_their_branch(self):
        june = fines(2025, 6)
        # San Juan closes only Norte: due Tue 24 June moves to Wed 25 there
        self.assertNotIn(("Sofía Ramírez", "Pedro Páramo"), june)
        self.assertEqual(june[("Tomás Herrera", "El Aleph")], 25)

    def test_due_date_skips_holidays(self):
        loan = Loan("L1", "I1", "M1", "centro", date(2025, 2, 17))
        item = Item("I1", "T1", "book", "centro")
        member = Member("M1", "Laura", "adult", "centro")
        calendar = load_calendar("centro")
        self.assertEqual(due_date(loan, item, member, calendar), date(2025, 3, 5))
        self.assertEqual(fine_for(loan, item, member, date(2025, 3, 5)), 0)
        self.assertEqual(fine_for(loan, item, member, date(2025, 3, 6)), 25)


if __name__ == "__main__":
    unittest.main()
