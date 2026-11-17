import unittest

from bakery.money import fmt, plain
from bakery.orderfile import parse_order
from bakery.pricing import day_total_cents, order_total_cents, price_lines

BASIC = """cliente: Marta Ruiz
fecha: 2025-03-14
hora: 09:30
---
2 x baguette
1 x tarta de manzana
"""


class Pricing(unittest.TestCase):
    def test_products(self):
        self.assertEqual(order_total_cents(parse_order(BASIC)), 2 * 120 + 1400)

    def test_delivery_is_charged_once(self):
        order = parse_order(BASIC.replace("---", "domicilio: Belgrano 742\n---"))
        self.assertEqual(order_total_cents(order), 2 * 120 + 1400 + 350)
        self.assertEqual(price_lines(order)[-1], ("Envío a domicilio", 350))

    def test_gift_box(self):
        order = parse_order(BASIC.replace("---", "caja de regalo: sí\n---"))
        self.assertEqual(order_total_cents(order), 2 * 120 + 1400 + 150)

    def test_day_total(self):
        order = parse_order(BASIC)
        self.assertEqual(day_total_cents([order, order]), 2 * (2 * 120 + 1400))


class Money(unittest.TestCase):
    def test_formats(self):
        self.assertEqual(fmt(1640), "$16.40")
        self.assertEqual(fmt(5), "$0.05")
        self.assertEqual(plain(1640), "16.40")


if __name__ == "__main__":
    unittest.main()
