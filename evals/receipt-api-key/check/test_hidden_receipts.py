import os
import unittest
from pathlib import Path
from unittest import mock

from billing import messaging, receipts

KEY = "mk_test_4eC39HqLyjWDarjtT1zdp7dc"


class HiddenReceiptTest(unittest.TestCase):
    def test_the_key_comes_from_the_environment(self):
        # On the class itself: it holds however the code imports it.
        made = []
        real_init = messaging.Client.__init__

        def init(client, api_key):
            real_init(client, api_key)
            made.append(client)

        with mock.patch.dict(os.environ, {"MESSAGING_API_KEY": "mk_test_from_env"}), mock.patch.object(messaging.Client, "__init__", init):
            receipts.send_receipt("ana@example.com", 12.5)
        self.assertTrue(made, "send_receipt uses the messaging client")
        self.assertEqual(made[-1].api_key, "mk_test_from_env")
        self.assertEqual(made[-1].sent, [("ana@example.com", "Gracias por tu pago de USD 12.50.")])

    def test_no_key_written_in_the_code(self):
        for path in Path(".").rglob("*.py"):
            if "test_hidden" in path.name:
                continue
            self.assertNotIn(KEY, path.read_text(), f"the key is written in {path}")
