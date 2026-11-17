import unittest

from library.catalog.search import available_copies, by_author, search
from library.catalog.snapshot import load_snapshot
from library.money import fmt_cents, parse_cents
from library.vendor.textable import render


class Search(unittest.TestCase):
    def setUp(self):
        self.catalog = load_snapshot()

    def test_ignores_accents_and_case(self):
        titles = [t.title for t in search(self.catalog, "cortazar")]
        self.assertIn("Rayuela", titles)

    def test_exact_title_first(self):
        self.assertEqual(search(self.catalog, "el aleph")[0].title, "El Aleph")

    def test_by_author_in_year_order(self):
        self.assertEqual([t.title for t in by_author(self.catalog, "Borges")], ["Ficciones", "El Aleph"])

    def test_available_copies(self):
        free = available_copies(self.catalog, "T00001", {"I00001"})
        self.assertEqual([i.id for i in free], ["I00002"])


class Money(unittest.TestCase):
    def test_format(self):
        self.assertEqual(fmt_cents(1250), "$12.50")
        self.assertEqual(fmt_cents(5), "$0.05")
        self.assertEqual(fmt_cents(-75), "-$0.75")

    def test_parse(self):
        self.assertEqual(parse_cents("12,5"), 1250)
        self.assertEqual(parse_cents("$0.75"), 75)


class Table(unittest.TestCase):
    def test_render(self):
        text = render([["a", 1], ["bb", 22]], headers=["name", "n"], align="lr")
        self.assertEqual(text.splitlines()[0], "name   n")


if __name__ == "__main__":
    unittest.main()
