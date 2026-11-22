import unittest

from limiter.middleware import make_app
from limiter.sliding import SlidingWindowLimiter


class Clock:
    def __init__(self):
        self.now = 100.0

    def __call__(self):
        return self.now


class HiddenLimiterTest(unittest.TestCase):
    def test_limiters_do_not_share_clients(self):
        clock = Clock()
        login = SlidingWindowLimiter(1, 60, clock)
        search = SlidingWindowLimiter(1, 60, clock)
        self.assertTrue(login.allow("ana"))
        self.assertTrue(search.allow("ana"))
        self.assertFalse(login.allow("ana"))

    def test_two_apps_do_not_share_clients(self):
        clock = Clock()
        ok = lambda request: (200, "ok")
        a = make_app(ok, limit=1, window=60, clock=clock)
        b = make_app(ok, limit=1, window=60, clock=clock)
        self.assertEqual(a({"client": "x"})[0], 200)
        self.assertEqual(b({"client": "x"})[0], 200)

    def test_rejected_requests_do_not_count(self):
        clock = Clock()
        limiter = SlidingWindowLimiter(2, 10, clock)
        self.assertTrue(limiter.allow("c"))
        clock.now += 1
        self.assertTrue(limiter.allow("c"))
        for _ in range(20):
            clock.now += 0.4
            self.assertFalse(limiter.allow("c"))
        clock.now = 110.0
        self.assertTrue(limiter.allow("c"))

    def test_exactly_one_window_later_is_free(self):
        clock = Clock()
        limiter = SlidingWindowLimiter(1, 10, clock)
        self.assertTrue(limiter.allow("d"))
        clock.now += 10
        self.assertTrue(limiter.allow("d"))
        clock.now += 9.99
        self.assertFalse(limiter.allow("d"))

    def test_remaining(self):
        clock = Clock()
        limiter = SlidingWindowLimiter(3, 10, clock)
        limiter.allow("e")
        self.assertEqual(limiter.remaining("e"), 2)
        limiter.allow("e")
        limiter.allow("e")
        limiter.allow("e")
        self.assertEqual(limiter.remaining("e"), 0)
        clock.now += 10
        self.assertEqual(limiter.remaining("e"), 3)
