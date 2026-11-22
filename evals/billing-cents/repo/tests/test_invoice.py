import unittest

from billing.cart import Cart
from billing.invoice import invoice_text


class InvoiceTest(unittest.TestCase):
    def test_simple_invoice(self):
        cart = Cart()
        cart.add("lamp", 2)
        self.assertEqual(
            invoice_text(cart, "OR"),
            "2 x lamp: $39.98\nSubtotal: $39.98\nImpuesto: $0.00\nTotal: $39.98",
        )

    def test_coupon(self):
        cart = Cart()
        cart.add("chair")
        text = invoice_text(cart, "OR", "SAVE5")
        self.assertIn("Descuento: -$5.00", text)
        self.assertTrue(text.endswith("Total: $84.95"))
