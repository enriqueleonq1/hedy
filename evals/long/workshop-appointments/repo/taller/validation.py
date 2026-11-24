"""Formas válidas de los datos que entran por el mostrador."""
import re

from .errors import ValidationError

_PHONE = re.compile(r"\d{8,13}")
_EMAIL = re.compile(r"[^@\s]+@[^@\s]+\.[a-z]{2,}", re.IGNORECASE)


def normalize_plate(text: str) -> str:
    """La patente tal como se guarda: sin espacios a los lados y en mayúsculas."""
    plate = (text or "").strip().upper()
    if not plate:
        raise ValidationError("Patente vacía")
    return plate


def normalize_phone(text: str) -> str:
    """Solo dígitos; vacío si no hay teléfono."""
    digits = re.sub(r"\D", "", text or "")
    if digits and not _PHONE.fullmatch(digits):
        raise ValidationError(f"Teléfono inválido: {text}")
    return digits


def check_email(text: str) -> str:
    text = (text or "").strip()
    if text and not _EMAIL.fullmatch(text):
        raise ValidationError(f"Correo inválido: {text}")
    return text


def check_year(year: int) -> int:
    if not 1950 <= int(year) <= 2030:
        raise ValidationError(f"Año inválido: {year}")
    return int(year)
