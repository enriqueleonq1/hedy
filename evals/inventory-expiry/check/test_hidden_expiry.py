import unittest

from shop import errors
from shop.inventory import Inventory
from shop.report import pending_report

MIN = 60


class Clock:
    def __init__(self):
        self.now = 1000.0

    def __call__(self):
        return self.now


class HiddenExpiryTest(unittest.TestCase):
    def setUp(self):
        self.clock = Clock()
        self.inv = Inventory(self.clock)
        self.inv.add_product("A1", "Lámpara", 5)
        self.inv.add_product("B2", "Silla", 2)

    def test_expired_stock_is_available_again(self):
        self.inv.reserve("A1", 4)
        self.clock.now += 14 * MIN + 59
        self.assertEqual(self.inv.available("A1"), 1)
        self.clock.now += 1
        self.assertEqual(self.inv.available("A1"), 5)
        self.inv.reserve("A1", 5)

    def test_release_expired_counts_once(self):
        self.inv.reserve("A1", 1)
        self.inv.reserve("B2", 1)
        self.clock.now += 10 * MIN
        self.inv.reserve("A1", 1)
        self.clock.now += 5 * MIN
        self.assertEqual(self.inv.release_expired(), 2)
        self.assertEqual(self.inv.release_expired(), 0)
        self.assertEqual(len(self.inv.reservations()), 1)

    def test_confirm_expired_fails_and_keeps_stock(self):
        self.assertTrue(issubclass(errors.ReservationExpired, errors.InventoryError))
        r = self.inv.reserve("A1", 2)
        self.clock.now += 15 * MIN
        with self.assertRaises(errors.ReservationExpired):
            self.inv.confirm(r.id)
        self.assertEqual(self.inv.product("A1").stock, 5)

    def test_confirmed_never_expire(self):
        r = self.inv.reserve("A1", 2)
        self.inv.confirm(r.id)
        self.clock.now += 60 * MIN
        self.assertEqual(self.inv.release_expired(), 0)
        self.assertEqual(self.inv.product("A1").stock, 3)
        self.assertEqual(self.inv.available("A1"), 3)
        self.assertEqual(self.inv.confirm(r.id).confirmed, True)

    def test_report_hides_expired(self):
        self.inv.reserve("A1", 1)
        self.clock.now += 10 * MIN
        self.inv.reserve("B2", 1)
        self.clock.now += 5 * MIN
        self.assertEqual(pending_report(self.inv), "#2 Silla x1")
