import unittest

from app.routes import handle


class RoutesTest(unittest.TestCase):
    def test_health(self):
        self.assertEqual(handle("/health"), (200, "ok"))

    def test_invoice(self):
        self.assertEqual(handle("/invoices/1001"), (200, "Acme: 120.00"))
        self.assertEqual(handle("/invoices/10%30%31"), (200, "Acme: 120.00"))
        self.assertEqual(handle("/invoices/9"), (404, "not found"))
