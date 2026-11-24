import pytest
from marshmallow import ValidationError
from marshmallow.validate import Even


def test_even_passes():
    assert Even()(6) == 6


@pytest.mark.parametrize("value", [1, "2", 2.5, None])
def test_not_even(value):
    with pytest.raises(ValidationError) as e:
        Even()(value)
    assert "Must be an even number." in str(e.value)


def test_own_error():
    with pytest.raises(ValidationError) as e:
        Even(error="par")(3)
    assert "par" in str(e.value)
