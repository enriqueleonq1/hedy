"""Reads the .pedido files the counter writes.

A file has header lines `campo: valor`, a `---` line, and one `N x producto`
per line:

    cliente: Marta Ruiz
    fecha: 2025-03-14
    hora: 09:30
    nota: sin azúcar glas
    ---
    2 x baguette
    1 x tarta de manzana
"""
from datetime import date, time

from .models import Line, Order


class OrderFileError(ValueError):
    pass


def _text(value: str) -> str:
    return value.strip()


def _date(value: str) -> date:
    try:
        return date.fromisoformat(value.strip())
    except ValueError:
        raise OrderFileError(f"fecha inválida: {value.strip()}") from None


def _time(value: str) -> time:
    try:
        hours, minutes = value.strip().split(":")
        return time(int(hours), int(minutes))
    except ValueError:
        raise OrderFileError(f"hora inválida: {value.strip()}") from None


def _yes_no(value: str) -> bool:
    answer = value.strip().lower()
    if answer in ("sí", "si"):
        return True
    if answer == "no":
        return False
    raise OrderFileError(f"se esperaba sí o no: {value.strip()}")


# Header key in the file -> (Order field, parser).
HEADER_FIELDS = {
    "cliente": ("customer", _text),
    "fecha": ("pickup_date", _date),
    "hora": ("pickup_time", _time),
    "nota": ("note", _text),
    "domicilio": ("delivery_address", _text),
    "caja de regalo": ("gift_box", _yes_no),
    "pagado": ("paid", _yes_no),
}
REQUIRED = ("cliente", "fecha", "hora")


def _parse_line(raw: str) -> Line:
    qty, sep, product = raw.partition(" x ")
    if not sep or not qty.strip().isdigit():
        raise OrderFileError(f"línea inválida: {raw}")
    return Line(int(qty), product.strip().lower())


def parse_order(text: str, source: str = "") -> Order:
    head, sep, body = text.partition("\n---")
    if not sep:
        raise OrderFileError("falta la línea ---")
    values = {}
    for raw in head.splitlines():
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        key, colon, value = raw.partition(":")
        key = key.strip().lower()
        if not colon or key not in HEADER_FIELDS:
            raise OrderFileError(f"campo desconocido: {key}")
        name, parser = HEADER_FIELDS[key]
        values[name] = parser(value)
    for key in REQUIRED:
        if HEADER_FIELDS[key][0] not in values:
            raise OrderFileError(f"falta el campo {key}")
    lines = [_parse_line(raw.strip()) for raw in body.splitlines() if raw.strip() and raw.strip() != "-"]
    if not lines:
        raise OrderFileError("el pedido no tiene productos")
    return Order(lines=lines, source=source, **values)
