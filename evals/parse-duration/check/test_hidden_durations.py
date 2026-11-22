import unittest

from timer.durations import parse_duration


class HiddenDurationTest(unittest.TestCase):
    def test_the_usual(self):
        self.assertEqual(parse_duration("1h30m"), 90)
        self.assertEqual(parse_duration("45m"), 45)
        self.assertEqual(parse_duration("2h"), 120)
        self.assertEqual(parse_duration("90m"), 90)

    def test_spaces_and_case(self):
        self.assertEqual(parse_duration("1h 30m"), 90)
        self.assertEqual(parse_duration(" 1H30M "), 90)

    def test_what_is_not_understood_is_refused(self):
        for bad in ["", "   ", "abc", "-5m", "1x", "h", "30", "1h30", "m30"]:
            with self.assertRaises(ValueError, msg=repr(bad)):
                parse_duration(bad)
