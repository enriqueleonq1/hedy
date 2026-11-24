"""Comprobaciones de lo que llega del mostrador o de la línea de comandos."""

import re

TIME = re.compile(r"^([01]\d|2[0-3]):[0-5]\d$")


def check_time(text):
    if not TIME.match(text or ""):
        raise ValueError(f"Hora inválida: {text}")
    return text


def check_qty(qty):
    if not isinstance(qty, int) or qty <= 0:
        raise ValueError(f"Cantidad inválida: {qty}")
    return qty


def check_name(name):
    name = " ".join((name or "").split())
    if not name:
        raise ValueError("Falta el nombre")
    return name
