import unittest
from datetime import date, time

from bakery.models import Line
from bakery.orderfile import OrderFileError, parse_order
from bakery.validation import problems

BASIC = """cliente: Marta Ruiz
fecha: 2025-03-14
hora: 09:30
---
2 x baguette
1 x Tarta de manzana
"""


class ParseOrder(unittest.TestCase):
    def test_required_fields_and_lines(self):
        order = parse_order(BASIC)
        self.assertEqual(order.customer, "Marta Ruiz")
        self.assertEqual(order.pickup_date, date(2025, 3, 14))
        self.assertEqual(order.pickup_time, time(9, 30))
        self.assertEqual(order.lines, [Line(2, "baguette"), Line(1, "tarta de manzana")])
        self.assertIsNone(order.note)
        self.assertFalse(order.gift_box)

    def test_optional_fields(self):
        order = parse_order(BASIC.replace("---", "domicilio: Belgrano 742\ncaja de regalo: sí\npagado: no\n---"))
        self.assertEqual(order.delivery_address, "Belgrano 742")
        self.assertTrue(order.gift_box)
        self.assertFalse(order.paid)

    def test_unknown_field(self):
        with self.assertRaises(OrderFileError):
            parse_order(BASIC.replace("---", "color: rojo\n---"))

    def test_missing_required_field(self):
        with self.assertRaises(OrderFileError):
            parse_order(BASIC.replace("hora: 09:30\n", ""))

    def test_bad_yes_no(self):
        with self.assertRaises(OrderFileError):
            parse_order(BASIC.replace("---", "pagado: tal vez\n---"))


class Validation(unittest.TestCase):
    def test_valid_order(self):
        self.assertEqual(problems(parse_order(BASIC)), [])

    def test_unknown_product_and_closed(self):
        order = parse_order(BASIC.replace("hora: 09:30", "hora: 23:00").replace("baguette", "pizza"))
        self.assertEqual(len(problems(order)), 2)

    def test_note_too_long(self):
        order = parse_order(BASIC.replace("---", "nota: " + "x" * 201 + "\n---"))
        self.assertEqual(problems(order), ["note supera 200 caracteres"])

    def test_cakes_need_notice(self):
        order = parse_order(BASIC.replace("Tarta de manzana", "torta de chocolate"))
        self.assertEqual(len(problems(order, received_on=date(2025, 3, 14))), 1)
        self.assertEqual(problems(order, received_on=date(2025, 3, 13)), [])


if __name__ == "__main__":
    unittest.main()
