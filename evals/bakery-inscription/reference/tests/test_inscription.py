import contextlib
import csv
import io
import pathlib
import tempfile
import unittest

from bakery.cli import main
from bakery.export import to_csv
from bakery.orderfile import parse_order
from bakery.pricing import order_total_cents
from bakery.ticket import render_ticket

BASIC = """cliente: Marta Ruiz
fecha: 2025-03-14
hora: 09:30
---
2 x baguette
"""
WITH = BASIC.replace("---", "dedicatoria: Feliz cumple, Ana\n---")


class Inscription(unittest.TestCase):
    def test_parsed_and_charged(self):
        order = parse_order(WITH)
        self.assertEqual(order.inscription, "Feliz cumple, Ana")
        self.assertEqual(order_total_cents(order), order_total_cents(parse_order(BASIC)) + 200)

    def test_ticket_and_csv(self):
        self.assertIn("Dedicatoria: Feliz cumple, Ana", render_ticket(parse_order(WITH)))
        rows = list(csv.DictReader(io.StringIO(to_csv([parse_order(WITH), parse_order(BASIC)]))))
        self.assertEqual([r["dedicatoria"] for r in rows], ["Feliz cumple, Ana", ""])

    def test_list_filter(self):
        with tempfile.TemporaryDirectory() as folder:
            pathlib.Path(folder, "1.pedido").write_text(WITH, encoding="utf-8")
            pathlib.Path(folder, "2.pedido").write_text(BASIC.replace("Marta Ruiz", "Pedro Salas"), encoding="utf-8")
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                main(["list", folder, "--solo-dedicatoria"])
            self.assertIn("Marta Ruiz", out.getvalue())
            self.assertNotIn("Pedro Salas", out.getvalue())


if __name__ == "__main__":
    unittest.main()
