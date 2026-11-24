import io
import unittest
from datetime import date

from gym.app import Gym
from gym.cli import main
from gym.exports import members_csv
from gym.importer import import_members

from .helpers import make_folder, remove


class ToolsTest(unittest.TestCase):
    def setUp(self):
        self.folder = make_folder()
        self.gym = Gym.load(self.folder)

    def tearDown(self):
        remove(self.folder)

    def test_import_reports_bad_rows(self):
        path = self.folder / "nuevos.csv"
        path.write_text("id,name,email,plan\ns010,Rosa Vidal,rosa@example.com,BASIC\ns011,Sin Plan,x@example.com,gold\n", encoding="utf-8")
        report = import_members(self.gym.state, path)
        self.assertEqual(report.added, ["s010"])
        self.assertEqual(report.skipped, [(3, "Plan desconocido: gold")])

    def test_members_csv(self):
        self.assertEqual(members_csv(self.gym.state).splitlines()[0], "id,nombre,email,plan,familia")

    def test_cli_schedule(self):
        out = io.StringIO()
        main(["--datos", str(self.folder), "horario", "2025-03-12"], out=out)
        self.assertIn("  18:00–19:00 Yoga · Marta · Sala 2", out.getvalue())

    def test_payment_dates(self):
        self.gym.record_payment("s002", 100, date(2025, 3, 5))
        self.assertEqual(self.gym.state.payments[0].day, date(2025, 3, 5))


if __name__ == "__main__":
    unittest.main()
