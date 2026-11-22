import time
from collections import deque


class SlidingWindowLimiter:
    """At most `limit` requests per client in any `window` seconds."""

    _hits = {}

    def __init__(self, limit, window, clock=time.monotonic):
        self.limit = limit
        self.window = window
        self.clock = clock

    def _recent(self, client):
        hits = self._hits.setdefault(client, deque())
        cutoff = self.clock() - self.window
        while hits and hits[0] < cutoff:
            hits.popleft()
        return hits

    def allow(self, client):
        hits = self._recent(client)
        hits.append(self.clock())
        return len(hits) <= self.limit

    def remaining(self, client):
        return max(0, self.limit - len(self._recent(client)))
