import unittest

from shop.invoice import invoice_total, line_total


class HiddenInvoiceTest(unittest.TestCase):
    def check(self, value, expected):
        self.assertAlmostEqual(float(value), expected, places=9)

    def test_half_up_everywhere(self):
        self.check(line_total(2.675, 1), 2.68)
        self.check(line_total(1.005, 1), 1.01)
        self.check(line_total(0.045, 1), 0.05)
        self.check(line_total(1.115, 1), 1.12)

    def test_with_quantity_and_discount(self):
        self.check(line_total(19.99, 3, 15), 50.97)
        self.check(line_total(4.35, 3), 13.05)
        self.check(line_total(10.10, 1, 50), 5.05)

    def test_the_total_is_the_sum_of_rounded_lines(self):
        lines = [{"price": 0.335, "qty": 1}, {"price": 0.335, "qty": 1}, {"price": 0.335, "qty": 1}]
        self.check(invoice_total(lines), 1.02)
        self.check(invoice_total([]), 0)
