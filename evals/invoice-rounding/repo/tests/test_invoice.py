import unittest

from shop.invoice import invoice_total, line_total


class InvoiceTest(unittest.TestCase):
    def test_a_plain_line(self):
        self.assertEqual(line_total(10, 3), 30)

    def test_half_rounds_up(self):
        self.assertAlmostEqual(float(line_total(1.005, 1)), 1.01)
        self.assertAlmostEqual(float(line_total(0.125, 1)), 0.13)

    def test_the_total_adds_rounded_lines(self):
        lines = [{"price": 0.335, "qty": 1}, {"price": 0.335, "qty": 1}]
        self.assertAlmostEqual(float(invoice_total(lines)), 0.68)
