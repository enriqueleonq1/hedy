import unittest

from shop.errors import NotEnoughStock
from shop.inventory import Inventory
from shop.report import pending_report


class Clock:
    def __init__(self):
        self.now = 1000.0

    def __call__(self):
        return self.now


class InventoryTest(unittest.TestCase):
    def setUp(self):
        self.clock = Clock()
        self.inv = Inventory(self.clock)
        self.inv.add_product("A1", "Lámpara", 5)

    def test_reserve_holds_stock(self):
        self.inv.reserve("A1", 3)
        self.assertEqual(self.inv.available("A1"), 2)
        with self.assertRaises(NotEnoughStock):
            self.inv.reserve("A1", 3)

    def test_confirm_takes_stock(self):
        r = self.inv.reserve("A1", 2)
        self.inv.confirm(r.id)
        self.assertEqual(self.inv.product("A1").stock, 3)
        self.assertEqual(self.inv.available("A1"), 3)

    def test_report(self):
        self.inv.reserve("A1", 1)
        self.assertEqual(pending_report(self.inv), "#1 Lámpara x1")
