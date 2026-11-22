from .catalog import price


class Cart:
    def __init__(self):
        self.lines = []

    def add(self, sku, qty=1):
        self.lines.append((sku, qty))

    def line_totals(self):
        return [(sku, qty, price(sku) * qty) for sku, qty in self.lines]

    def subtotal(self):
        return sum(total for _, _, total in self.line_totals())
