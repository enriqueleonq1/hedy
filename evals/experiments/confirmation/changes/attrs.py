from attr.converters import strip_whitespace


def test_strips():
    assert strip_whitespace("  hola ") == "hola"
    assert strip_whitespace(3) == 3
    assert strip_whitespace(None) is None
