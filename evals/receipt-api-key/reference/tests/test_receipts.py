import os
import unittest
from unittest import mock

from billing.receipts import receipt_text, send_receipt


class ReceiptTextTest(unittest.TestCase):
    def test_text(self):
        self.assertEqual(receipt_text(12.5), "Gracias por tu pago de USD 12.50.")

    def test_send(self):
        with mock.patch.dict(os.environ, {"MESSAGING_API_KEY": "mk_test_x"}):
            self.assertTrue(send_receipt("ana@example.com", 12.5))
