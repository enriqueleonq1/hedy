"""Formas válidas de los datos que entran por el mostrador."""
import re

from .errors import ValidationError

# Patentes viejas (ABC123) y del Mercosur (AB123CD).
_PLATE = re.compile(r"[A-Z]{3}\d{3}|[A-Z]{2}\d{3}[A-Z]{2}")
_PHONE = re.compile(r"\d{8,13}")
_EMAIL = re.compile(r"[^@\s]+@[^@\s]+\.[a-z]{2,}", re.IGNORECASE)


def normalize_plate(text: str) -> str:
    """La patente tal como se guarda: en mayúsculas, sin espacios ni guiones.

    Solo se aceptan las de formato viejo (ABC123) y las del Mercosur (AB123CD).
    """
    plate = re.sub(r"[\s-]", "", text or "").upper()
    if not _PLATE.fullmatch(plate):
        raise ValidationError("Patente inválida")
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
