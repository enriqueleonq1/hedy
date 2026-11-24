import pytest
from voluptuous import Invalid, Schema
from voluptuous.validators import Even


def test_even_passes():
    assert Even()(4) == 4
    assert Even()(0) == 0


@pytest.mark.parametrize("value", [3, "4", 2.0, None])
def test_not_even(value):
    with pytest.raises(Invalid) as e:
        Even()(value)
    assert "expected an even number" in str(e.value)


def test_own_message():
    with pytest.raises(Invalid) as e:
        Even(msg="par, por favor")(5)
    assert "par, por favor" in str(e.value)
