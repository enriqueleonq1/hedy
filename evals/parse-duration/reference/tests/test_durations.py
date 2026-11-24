import unittest

from timer.durations import format_minutes, parse_duration


class FormatTest(unittest.TestCase):
    def test_formats(self):
        self.assertEqual(format_minutes(90), "1h30m")
        self.assertEqual(format_minutes(45), "45m")


class ParseTest(unittest.TestCase):
    def test_parses(self):
        self.assertEqual(parse_duration("1h30m"), 90)
        self.assertEqual(parse_duration("1H 30M"), 90)
        self.assertEqual(parse_duration("2h"), 120)

    def test_refuses(self):
        for bad in ["", "abc", "-5m", "1x"]:
            with self.assertRaises(ValueError):
                parse_duration(bad)
