import ast
import unittest
from pathlib import Path

from store.shipping import domestic_cost, international_cost


class HiddenShippingTest(unittest.TestCase):
    def test_same_prices_as_before(self):
        self.assertEqual(domestic_cost(49.99, 1), 5.5)
        self.assertEqual(domestic_cost(50, 30), 0)
        self.assertEqual(domestic_cost(10, 11), 28.5)
        self.assertEqual(domestic_cost(10, 10), 19)
        self.assertEqual(international_cost(100, 1), 19)
        self.assertEqual(international_cost(100.01, 9), 0)
        self.assertEqual(international_cost(10, 6), 59)
        self.assertEqual(international_cost(10, 5), 35)

    def test_one_shared_logic(self):
        source = Path("store/shipping.py").read_text()
        tree = ast.parse(source)
        ifs = sum(isinstance(n, ast.If) for n in ast.walk(tree))
        self.assertLessEqual(ifs, 2, "the threshold and surcharge checks live once")
