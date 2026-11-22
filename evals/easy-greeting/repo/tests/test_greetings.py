import unittest

from app.greetings import goodbye


class GreetingTest(unittest.TestCase):
    def test_goodbye(self):
        self.assertEqual(goodbye(), "Hasta pronto")
