"""Consumos del bar, impresiones y taquillas."""

from .dates import month_key
from .models import Consumption
from .validation import check_qty


def consume(store, member_id, code, qty, day):
    """Registra un consumo de un socio."""
    from .members import find_member

    find_member(store, member_id)
    extra = store.extras[code]
    check_qty(qty)
    item = Consumption(member_id, extra.code, qty, day)
    store.consumptions.append(item)
    return item


def consumptions_in_month(store, member_id, month):
    return [c for c in store.consumptions if c.member == member_id and month_key(c.day) == month]


def extras_text(store):
    return "\n".join(f"{e.code}: {e.name}" for e in store.extras.values())
