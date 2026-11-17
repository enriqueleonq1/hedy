import io
import unittest
from datetime import date

from library.cli import main
from library.reports.circulation import monthly_circulation
from library.reports.fines_report import monthly_fines
from library.reports.overdue import overdue_on
from library.storage.csv_store import load_library


class FinesReport(unittest.TestCase):
    def setUp(self):
        self.library = load_library()

    def test_march(self):
        fines = {(r.member, r.title): r.cents for r in monthly_fines(self.library, 2025, 3)}
        self.assertEqual(fines[("Diego Fernández", "Ficciones")], 100)
        self.assertEqual(fines[("Martín Acosta", "Metrópolis")], 300)
        # Carmen returned on time; Lucía is staff
        self.assertNotIn("Carmen Ortiz", {m for m, _ in fines})
        self.assertNotIn("Lucía Navarro", {m for m, _ in fines})

    def test_february_cap(self):
        fines = {(r.member, r.title): r.cents for r in monthly_fines(self.library, 2025, 2)}
        self.assertEqual(fines[("Joaquín Molina", "Metrópolis")], 1000)

    def test_cli_prints_the_table(self):
        out = io.StringIO()
        self.assertEqual(main(["report", "fines", "2025-03"], out=out), 0)
        text = out.getvalue()
        self.assertIn("Multas de 03/2025", text)
        self.assertIn("Diego Fernández", text)
        self.assertIn("$1.00", text)


class OtherReports(unittest.TestCase):
    def setUp(self):
        self.library = load_library()

    def test_circulation(self):
        march = monthly_circulation(self.library, 2025, 3)
        self.assertEqual(march["centro"]["dvd"], 1)
        self.assertEqual(sum(march["puerto"].values()), 2)

    def test_overdue(self):
        rows = overdue_on(self.library, date(2025, 6, 30))
        self.assertIn("Joaquín Molina", [r.member for r in rows])


if __name__ == "__main__":
    unittest.main()
