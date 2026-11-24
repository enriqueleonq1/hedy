import pytest
from bitstring.utils import popcount_hex


def test_counts_bits():
    assert popcount_hex("0xff") == 8
    assert popcount_hex("0XF0") == 4
    assert popcount_hex("10") == 1
    assert popcount_hex("") == 0


def test_not_hex():
    with pytest.raises(ValueError):
        popcount_hex("0xzz")
