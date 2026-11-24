"""Resumen mensual."""

import logging
from decimal import Decimal

logger = logging.getLogger(__name__)


def monthly_summary(sales, month):
    """Lo vendido en `month` ('AAAA-MM'), registrado para el equipo de ventas."""
    total = sum(
        (Decimal(str(s["amount"])) for s in sales if s["date"].startswith(f"{month}-")),
        Decimal("0"),
    )
    logger.info("Ventas de %s: %s", month, total)
    return total
