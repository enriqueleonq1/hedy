import ast
import io
import logging
import unittest
from contextlib import redirect_stdout
from decimal import Decimal
from pathlib import Path

from reports.summary import monthly_summary

SALES = [
    {"date": "2025-03-01", "amount": "0.10"},
    {"date": "2025-03-15", "amount": "0.20"},
    {"date": "2025-03-31", "amount": "10"},
    {"date": "2025-04-01", "amount": "99"},
]


class HiddenSummaryTest(unittest.TestCase):
    def test_the_total_of_the_month_exactly(self):
        total = monthly_summary(SALES, "2025-03")
        self.assertEqual(Decimal(str(total)), Decimal("10.30"))
        self.assertIsInstance(total, Decimal, "money is Decimal (the project's rule)")

    def test_it_logs_and_never_prints(self):
        out = io.StringIO()
        with redirect_stdout(out), self.assertLogs("reports.summary", level="INFO"):
            monthly_summary(SALES, "2025-03")
        self.assertEqual(out.getvalue(), "", "reports/ never prints (the project's rule)")

    def test_no_print_or_float_in_the_module(self):
        tree = ast.parse(Path("reports/summary.py").read_text())
        names = {n.func.id for n in ast.walk(tree) if isinstance(n, ast.Call) and isinstance(n.func, ast.Name)}
        self.assertNotIn("print", names)
        self.assertNotIn("float", names)
