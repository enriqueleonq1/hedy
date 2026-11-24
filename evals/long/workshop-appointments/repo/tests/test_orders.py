import unittest

from taller import invoices, money
from taller.errors import ShopError
from tests.helpers import fresh_shop


class OrdersTest(unittest.TestCase):
    def setUp(self):
        self.shop, self.path = fresh_shop()

    def test_close_takes_the_parts_and_sends_a_message(self):
        order = self.shop.open_order("AB123CD", ["OIL"], "M1", "2025-03-10")
        self.shop.close_order(order.id, "2025-03-10")
        self.assertEqual(self.shop.inventory.stock("FIL-ACE"), 5)
        self.assertEqual(self.shop.inventory.stock("ACE-5W30"), 16)
        # 15.000 + 8.500 + 4 x 12.000 = 71.500, + 21 % = 86.515
        self.assertEqual(order.total_cents, 8651500)
        self.assertEqual(self.shop.outbox[-1].to, "1155550001")
        self.assertIn("ya está listo", self.shop.outbox[-1].text)

    def test_not_enough_stock(self):
        order = self.shop.open_order("AC456EF", ["BRK"], "M1", "2025-03-10")
        self.shop.add_part(order.id, "PAS-DEL", 1)
        with self.assertRaises(ShopError):
            self.shop.close_order(order.id, "2025-03-10")
        self.assertEqual(self.shop.inventory.stock("PAS-DEL"), 1)

    def test_mechanic_must_have_the_skill(self):
        with self.assertRaises(ShopError):
            self.shop.open_order("AB123CD", ["BRK"], "M2")

    def test_invoice(self):
        text = invoices.invoice_text(self.shop, 2)
        self.assertTrue(text.startswith("Factura T-0002 — 2025-02-18"))
        self.assertIn(money.fmt(9317000), text)

    def test_money(self):
        self.assertEqual(money.fmt(1234550), "$ 12.345,50")
        self.assertEqual(money.parse("12.345,50"), 1234550)
        self.assertEqual(money.vat(1000), 210)


if __name__ == "__main__":
    unittest.main()
