"""Reporte diario."""

import logging
from decimal import Decimal

logger = logging.getLogger(__name__)


def daily_total(sales, day):
    """Lo vendido en `day` ('AAAA-MM-DD')."""
    total = sum((Decimal(str(s["amount"])) for s in sales if s["date"] == day), Decimal("0"))
    logger.info("Ventas del %s: %s", day, total)
    return total
