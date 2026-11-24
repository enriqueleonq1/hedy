from .money import ratio

# Rates in hundred-thousandths.
RATES = {"NY": 8875, "TX": 6250, "OR": 0}


def tax(amount, region):
    return ratio(amount, RATES[region], 100000)
