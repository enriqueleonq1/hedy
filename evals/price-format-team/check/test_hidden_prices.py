import unittest

from shop.catalog import Product, format_price


class FormatPrice(unittest.TestCase):
    def test_project_convention(self):
        self.assertEqual(format_price(Product("x", 1250)), "USD 12.50")
        self.assertEqual(format_price(Product("x", 5)), "USD 0.05")
        self.assertEqual(format_price(Product("x", 99900)), "USD 999.00")


if __name__ == "__main__":
    unittest.main()
