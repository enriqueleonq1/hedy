import unittest

from store.shipping import domestic_cost, international_cost


class ShippingTest(unittest.TestCase):
    def test_domestic(self):
        self.assertEqual(domestic_cost(20, 2), 7)
        self.assertEqual(domestic_cost(50, 2), 0)

    def test_international(self):
        self.assertEqual(international_cost(20, 2), 23)
