# Unit prices, in cents.
PRICES = {"pen": 115, "notebook": 235, "stapler": 705, "lamp": 1999, "chair": 8995}


def price(sku):
    return PRICES[sku]
