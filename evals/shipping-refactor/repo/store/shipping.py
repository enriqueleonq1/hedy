"""Costo de envío de un pedido."""


def domestic_cost(subtotal, weight_kg):
    # Gratis desde 50 (incluido).
    if subtotal >= 50:
        return 0
    cost = 4 + 1.5 * weight_kg
    if weight_kg > 10:
        cost += 8
    return round(cost, 2)


def international_cost(subtotal, weight_kg):
    # Gratis solo por encima de 100 (100 exacto paga).
    if subtotal > 100:
        return 0
    cost = 15 + 4 * weight_kg
    if weight_kg > 5:
        cost += 20
    return round(cost, 2)
