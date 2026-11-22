import unittest

from timer.durations import format_minutes


class FormatTest(unittest.TestCase):
    def test_formats(self):
        self.assertEqual(format_minutes(90), "1h30m")
        self.assertEqual(format_minutes(45), "45m")
