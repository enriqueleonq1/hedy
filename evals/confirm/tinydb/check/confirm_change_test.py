import pytest
from tinydb.utils import chunked


def test_chunks():
    assert chunked([1, 2, 3], 2) == [[1, 2], [3]]
    assert chunked(range(4), 2) == [[0, 1], [2, 3]]
    assert chunked([], 3) == []


def test_bad_size():
    with pytest.raises(ValueError):
        chunked([1], 0)
