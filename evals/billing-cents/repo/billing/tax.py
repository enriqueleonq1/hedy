RATES = {
    "NY": 0.08875,
    "TX": 0.0625,
    "OR": 0.0,
}


def tax(amount, region):
    return round(amount * RATES[region], 2)
