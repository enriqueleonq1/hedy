import unittest

from limiter.sliding import SlidingWindowLimiter


class Clock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        return self.now


class LimiterTest(unittest.TestCase):
    def test_blocks_over_the_limit(self):
        clock = Clock()
        limiter = SlidingWindowLimiter(2, 10, clock)
        self.assertTrue(limiter.allow("a"))
        self.assertTrue(limiter.allow("a"))
        self.assertFalse(limiter.allow("a"))

    def test_window_slides(self):
        clock = Clock()
        limiter = SlidingWindowLimiter(1, 10, clock)
        self.assertTrue(limiter.allow("b"))
        clock.now = 11
        self.assertTrue(limiter.allow("b"))
