import io
import unittest

from taller import history, reports
from taller.cli import main
from taller.shop import Shop
from tests.helpers import fresh_shop


class StorageTest(unittest.TestCase):
    def test_save_and_load_keep_everything(self):
        shop, path = fresh_shop()
        order = shop.open_order("KLM321", ["ALI"], "M2", "2025-03-11")
        shop.save()
        again = Shop.load(path)
        self.assertEqual(again.orders.get(order.id).services, ["ALI"])
        self.assertEqual(again.to_dict(), shop.to_dict())

    def test_monthly_report(self):
        shop, _ = fresh_shop()
        text = reports.monthly_text(shop.orders, "2025-02")
        self.assertTrue(text.startswith("Facturación de febrero 2025"))
        self.assertIn("en 3 órdenes", text)

    def test_next_oil_change(self):
        shop, _ = fresh_shop()
        due = history.next_due(shop.orders, "AC456EF")
        self.assertEqual(due["OIL"].isoformat(), "2025-08-03")


class CliTest(unittest.TestCase):
    def run_cli(self, *argv):
        _, path = fresh_shop()
        out = io.StringIO()
        code = main(["--data", str(path), *argv], out=out)
        return code, out.getvalue()

    def test_mechanics(self):
        code, text = self.run_cli("mecanicos")
        self.assertEqual(code, 0)
        self.assertIn("M1  Rosa Paz: general, frenos", text)

    def test_export_month(self):
        code, text = self.run_cli("exportar", "2025-02")
        self.assertEqual(code, 0)
        self.assertEqual(text.splitlines()[0], "orden,fecha,patente,cliente,servicios,total")
        self.assertIn("2,2025-02-18,AC456EF,Pedro Salas,BRK,93170.00", text)

    def test_low_stock(self):
        code, text = self.run_cli("repuestos", "--bajo-stock")
        self.assertEqual(code, 0)
        self.assertIn("PAS-DEL", text)
        self.assertNotIn("FIL-ACE", text)


if __name__ == "__main__":
    unittest.main()
