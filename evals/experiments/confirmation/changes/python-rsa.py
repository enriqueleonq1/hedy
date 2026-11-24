import pytest
from rsa.common import is_power_of_two


def test_powers():
    assert all(is_power_of_two(1 << k) for k in range(70))
    assert not any(is_power_of_two(n) for n in (0, -2, 3, 6, 12, (1 << 64) + 1))


def test_not_int():
    with pytest.raises(TypeError):
        is_power_of_two(2.0)
