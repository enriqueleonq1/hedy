import csv
import io
import unittest

from bakery.export import to_csv
from bakery.orderfile import parse_order
from bakery.ticket import render_ticket

BASIC = """cliente: Marta Ruiz
fecha: 2025-03-14
hora: 09:30
---
2 x baguette
1 x tarta de manzana
"""


class Ticket(unittest.TestCase):
    def test_lines_and_total(self):
        text = render_ticket(parse_order(BASIC))
        self.assertIn("Cliente: Marta Ruiz", text)
        self.assertIn("Retira: 14/03/2025 09:30", text)
        self.assertIn("2 x Baguette", text)
        self.assertIn("$16.40", text)
        self.assertIn("A PAGAR AL RETIRAR", text)

    def test_details_only_when_present(self):
        self.assertNotIn("Nota:", render_ticket(parse_order(BASIC)))
        text = render_ticket(parse_order(BASIC.replace("---", "nota: sin azúcar glas\n---")))
        self.assertIn("Nota: sin azúcar glas", text)


class Csv(unittest.TestCase):
    def test_columns(self):
        order = parse_order(BASIC.replace("---", "domicilio: Belgrano 742\n---"), source="001.pedido")
        rows = list(csv.DictReader(io.StringIO(to_csv([order]))))
        self.assertEqual(rows[0]["archivo"], "001.pedido")
        self.assertEqual(rows[0]["total"], "19.90")
        self.assertEqual(rows[0]["domicilio"], "Belgrano 742")
        self.assertEqual(rows[0]["caja_regalo"], "no")


if __name__ == "__main__":
    unittest.main()
