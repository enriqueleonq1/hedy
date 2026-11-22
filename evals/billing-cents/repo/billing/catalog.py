# Unit prices, in dollars.
PRICES = {
    "pen": 1.15,
    "notebook": 2.35,
    "stapler": 7.05,
    "lamp": 19.99,
    "chair": 89.95,
}


def price(sku):
    return PRICES[sku]
