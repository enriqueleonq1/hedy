import unittest

from billing.receipts import receipt_text


class ReceiptTextTest(unittest.TestCase):
    def test_text(self):
        self.assertEqual(receipt_text(12.5), "Gracias por tu pago de USD 12.50.")
