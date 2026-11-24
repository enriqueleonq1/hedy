import io
import unittest
from datetime import date

from cowork import access, cli, reports
from cowork.storage import Store
from tests.helpers import fresh_store


class AccessTest(unittest.TestCase):
    def test_only_members_in_alta_enter(self):
        store = fresh_store(self)
        access.enter(store, "S001", date(2025, 3, 10), "09:05")
        with self.assertRaises(ValueError):
            access.enter(store, "S004", date(2025, 3, 10), "09:05")
        self.assertEqual(access.visits_in_month(store, "S001", "2025-03"), 1)


class ReportsTest(unittest.TestCase):
    def test_month_report(self):
        store = fresh_store(self)
        text = reports.month_report(store, "2025-03")
        self.assertEqual(text.splitlines()[0], "Resumen de marzo 2025")
        self.assertIn("Socios activos: 3", text)
        self.assertIn("Altas: 1", text)


class StorageTest(unittest.TestCase):
    def test_saved_and_loaded(self):
        store = fresh_store(self)
        access.enter(store, "S001", date(2025, 3, 10), "09:05")
        store.save()
        again = Store.load(store.folder)
        self.assertEqual(len(again.access_log), 1)


class CliTest(unittest.TestCase):
    def test_socios(self):
        store = fresh_store(self)
        out = io.StringIO()
        self.assertEqual(cli.main(["--datos", str(store.folder), "socios"], out=out), 0)
        self.assertIn("S002 Bruno Sanz (Flex)", out.getvalue())


class MoreReportsTest(unittest.TestCase):
    def test_plan_mix_and_ranking(self):
        from cowork import extras

        store = fresh_store(self)
        self.assertIn("  Fijo: 1", reports.plan_mix_text(store, "2025-03"))
        extras.consume(store, "S001", "cafe", 3, date(2025, 3, 10))
        extras.consume(store, "S002", "impresion", 5, date(2025, 3, 11))
        self.assertEqual(reports.extras_ranking(store, "2025-03"), [("Impresión", 5), ("Café", 3)])
