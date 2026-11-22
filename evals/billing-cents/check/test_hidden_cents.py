import ast
import unittest
from decimal import ROUND_HALF_UP, Decimal
from pathlib import Path

from billing import catalog
from billing.cart import Cart
from billing.invoice import invoice_text

DOLLARS = {"pen": "1.15", "notebook": "2.35", "stapler": "7.05", "lamp": "19.99", "chair": "89.95"}
RATES = {"NY": "0.08875", "TX": "0.0625", "OR": "0"}
COUPONS = {None: None, "TEN": ("percent", 10), "SAVE5": ("fixed", "5"), "HALF": ("percent", 50), "BIG": ("fixed", "100")}
CENT = Decimal("0.01")


def expected(lines, region, coupon):
    """The rules, in exact decimals."""
    out = []
    subtotal = Decimal(0)
    for sku, qty in lines:
        total = Decimal(DOLLARS[sku]) * qty
        subtotal += total
        out.append(f"{qty} x {sku}: ${total:,.2f}")
    off = Decimal(0)
    if COUPONS[coupon]:
        kind, value = COUPONS[coupon]
        if kind == "percent":
            off = (subtotal * value / 100).quantize(CENT, ROUND_HALF_UP)
        else:
            off = min(Decimal(value), subtotal)
    taxed = subtotal - off
    tax = (taxed * Decimal(RATES[region])).quantize(CENT, ROUND_HALF_UP)
    out.append(f"Subtotal: ${subtotal:,.2f}")
    if off:
        out.append(f"Descuento: -${off:,.2f}")
    out.append(f"Impuesto: ${tax:,.2f}")
    out.append(f"Total: ${taxed + tax:,.2f}")
    return "\n".join(out)


class HiddenCentsTest(unittest.TestCase):
    def test_no_float_left(self):
        for path in Path("billing").glob("*.py"):
            tree = ast.parse(path.read_text())
            for node in ast.walk(tree):
                if isinstance(node, ast.Constant) and isinstance(node.value, float):
                    self.fail(f"{path}: float literal {node.value}")
                if isinstance(node, ast.Name) and node.id == "float":
                    self.fail(f"{path}: uses float()")
                if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Div):
                    self.fail(f"{path}:{node.lineno}: true division gives a float; use // or divmod")

    def test_catalog_in_cents(self):
        self.assertEqual(catalog.price("lamp"), 1999)
        self.assertIsInstance(catalog.price("pen"), int)

    def test_every_combination(self):
        carts = [
            [("pen", 3)],
            [("notebook", 7), ("pen", 1)],
            [("stapler", 3), ("lamp", 1)],
            [("chair", 1), ("pen", 9)],
            [("lamp", 111)],
            [("notebook", 1)],
            [("stapler", 1), ("notebook", 3), ("pen", 2)],
        ]
        for lines in carts:
            for region in RATES:
                for coupon in COUPONS:
                    cart = Cart()
                    for sku, qty in lines:
                        cart.add(sku, qty)
                    with self.subTest(lines=lines, region=region, coupon=coupon):
                        self.assertEqual(invoice_text(cart, region, coupon), expected(lines, region, coupon))
