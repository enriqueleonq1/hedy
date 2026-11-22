import unittest

from app.greetings import goodbye, welcome


class HiddenGreetingTest(unittest.TestCase):
    def test_welcome_names(self):
        self.assertEqual(welcome("Ana"), "Bienvenido, Ana!")
        self.assertEqual(goodbye(), "Hasta pronto")
