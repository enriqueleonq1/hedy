from .sliding import SlidingWindowLimiter


def make_app(handler, limit=5, window=60, clock=None):
    """Wraps handler(request) -> (status, body) with a per-client limit."""
    kwargs = {"clock": clock} if clock else {}
    limiter = SlidingWindowLimiter(limit, window, **kwargs)

    def app(request):
        client = request["client"]
        if not limiter.allow(client):
            return 429, "too many requests"
        return handler(request)

    return app
