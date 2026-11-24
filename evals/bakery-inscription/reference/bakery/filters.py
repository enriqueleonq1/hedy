"""Filters for `bakery list`: each one becomes a `--solo-<name>` flag."""
from .models import Order

FILTERS = {
    "pendientes": ("solo los que falta pagar", lambda o: not o.paid),
    "domicilio": ("solo los que van a domicilio", lambda o: o.delivery_address is not None),
    "regalo": ("solo los que llevan caja de regalo", lambda o: o.gift_box),
    "dedicatoria": ("solo los que llevan dedicatoria", lambda o: o.inscription is not None),
}


def apply(orders: list[Order], names: list[str]) -> list[Order]:
    """Orders that pass every filter in `names`."""
    keep = orders
    for name in names:
        _, test = FILTERS[name]
        keep = [o for o in keep if test(o)]
    return keep
