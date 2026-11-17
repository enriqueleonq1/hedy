import contextlib
import csv
import dataclasses
import io
import pathlib
import tempfile
import unicodedata
import unittest

from bakery.cli import main
from bakery.export import to_csv
from bakery.money import fmt
from bakery.orderfile import parse_order
from bakery.pricing import order_total_cents
from bakery.ticket import render_ticket

TEXT = "Feliz cumple, Ana"

PLAIN = """cliente: Marta Ruiz
fecha: 2025-03-14
hora: 18:30
---
1 x torta de chocolate
2 x medialuna
"""
WITH_TEXT = PLAIN.replace("---", f"dedicatoria: {TEXT}\n---")

OTHER = """cliente: Pedro Salas
fecha: 2025-03-14
hora: 11:00
domicilio: Belgrano 742
---
12 x medialuna
"""

# Accepted names for the new `list` filter, following the --solo-<name> pattern.
FLAGS = ["--solo-dedicatoria", "--solo-dedicatorias", "--solo-con-dedicatoria"]


def fold(text):
    decomposed = unicodedata.normalize("NFKD", text)
    return "".join(c for c in decomposed if not unicodedata.combining(c)).lower()


def run(*argv):
    out = io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
        code = main(list(argv))
    return code, out.getvalue()


class Inscription(unittest.TestCase):
    def test_parsed_into_the_order(self):
        order = parse_order(WITH_TEXT)
        values = [getattr(order, f.name) for f in dataclasses.fields(order)]
        self.assertIn(TEXT, values)

    def test_costs_two_pesos_once(self):
        base = order_total_cents(parse_order(PLAIN))
        self.assertEqual(order_total_cents(parse_order(WITH_TEXT)), base + 200)

    def test_ticket(self):
        order = parse_order(WITH_TEXT)
        text = render_ticket(order)
        self.assertIn(TEXT, text)
        self.assertIn(fmt(order_total_cents(order)), text)
        self.assertNotIn("dedicatoria", fold(render_ticket(parse_order(PLAIN))))

    def test_csv(self):
        orders = [parse_order(WITH_TEXT, source="a.pedido"), parse_order(OTHER, source="b.pedido")]
        reader = csv.DictReader(io.StringIO(to_csv(orders)))
        columns = [c for c in reader.fieldnames if "dedicatoria" in fold(c)]
        self.assertEqual(len(columns), 1, reader.fieldnames)
        # existing columns keep their names
        for name in ["archivo", "cliente", "fecha", "hora", "total", "pagado", "nota", "domicilio", "caja_regalo"]:
            self.assertIn(name, reader.fieldnames)
        rows = list(reader)
        self.assertEqual(rows[0][columns[0]], TEXT)
        self.assertEqual(rows[1][columns[0]], "")
        self.assertEqual(rows[0]["total"], "29.80")  # 26.00 + 2 x 0.90 + 2.00

    def test_list_filter(self):
        with tempfile.TemporaryDirectory() as folder:
            pathlib.Path(folder, "001.pedido").write_text(WITH_TEXT, encoding="utf-8")
            pathlib.Path(folder, "002.pedido").write_text(OTHER, encoding="utf-8")
            _, everything = run("list", folder)
            self.assertIn("Pedro Salas", everything)
            for flag in FLAGS:
                try:
                    code, text = run("list", folder, flag)
                except SystemExit:
                    continue
                self.assertEqual(code, 0)
                self.assertIn("Marta Ruiz", text)
                self.assertNotIn("Pedro Salas", text)
                return
            self.fail(f"list no acepta ninguno de {FLAGS}")


if __name__ == "__main__":
    unittest.main()
