import contextlib
import io
import pathlib
import unittest

from bakery.cli import main

SAMPLES = pathlib.Path(__file__).resolve().parent.parent / "samples"


def run(*argv):
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = main(list(argv))
    return code, out.getvalue()


class Cli(unittest.TestCase):
    def test_list_all(self):
        code, text = run("list", str(SAMPLES))
        self.assertEqual(code, 0)
        self.assertIn("4 pedidos", text)
        # sorted by pickup time: the club picks up first
        self.assertLess(text.index("Club Atlético Unión"), text.index("Marta Ruiz"))

    def test_filters(self):
        _, text = run("list", str(SAMPLES), "--solo-domicilio")
        self.assertIn("Pedro Salas", text)
        self.assertIn("Club Atlético Unión", text)
        self.assertNotIn("Marta Ruiz", text)
        _, text = run("list", str(SAMPLES), "--solo-regalo")
        self.assertIn("Inés Carrizo", text)
        self.assertIn("1 pedidos", text)
        _, text = run("list", str(SAMPLES), "--solo-pendientes", "--solo-domicilio")
        self.assertIn("Club Atlético Unión", text)
        self.assertNotIn("Pedro Salas", text)

    def test_ticket(self):
        code, text = run("ticket", str(SAMPLES / "2025-03-14-003.pedido"))
        self.assertEqual(code, 0)
        self.assertIn("Caja de regalo", text)

    def test_csv(self):
        _, text = run("csv", str(SAMPLES))
        self.assertTrue(text.startswith("archivo,cliente,fecha,hora,total,"))
        self.assertEqual(len(text.strip().splitlines()), 5)

    def test_check(self):
        code, text = run("check", str(SAMPLES))
        self.assertEqual(code, 0)
        self.assertIn("Todo en orden.", text)


if __name__ == "__main__":
    unittest.main()
