"""Costo de envío de un pedido."""


def _cost(free, base, per_kg, heavy_kg, surcharge, weight_kg):
    if free:
        return 0
    cost = base + per_kg * weight_kg
    if weight_kg > heavy_kg:
        cost += surcharge
    return round(cost, 2)


def domestic_cost(subtotal, weight_kg):
    # Gratis desde 50 (incluido).
    return _cost(subtotal >= 50, 4, 1.5, 10, 8, weight_kg)


def international_cost(subtotal, weight_kg):
    # Gratis solo por encima de 100 (100 exacto paga).
    return _cost(subtotal > 100, 15, 4, 5, 20, weight_kg)
