"""Helpers of the hidden checks: a fresh copy of the project's data/."""
import contextlib
import io
import shutil
import tempfile
import unittest
from datetime import date
from pathlib import Path

DATA = Path("data").resolve()


class Case(unittest.TestCase):
    def setUp(self):
        from cowork.storage import Store

        self.folder = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.folder, True)
        for item in DATA.iterdir():
            if item.is_file():
                shutil.copy(item, self.folder / item.name)
        self.store = Store.load(self.folder)

    def reload(self):
        from cowork.storage import Store

        self.store.save()
        self.store = Store.load(self.folder)
        return self.store

    def invoice(self, member, month="2025-03"):
        from cowork import billing

        return billing.invoice(self.store, member, month)

    def consume(self, member, code, qty, day=date(2025, 3, 10)):
        from cowork import extras

        return extras.consume(self.store, member, code, qty, day)

    def lines(self, item):
        return [(line.description, line.cents) for line in item.lines]

    def raises(self, message, call, *args):
        with self.assertRaises(ValueError) as caught:
            call(*args)
        self.assertEqual(str(caught.exception.args[0]), message)

    def cli(self, *argv):
        from cowork import cli

        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stderr(err):
            code = cli.main(["--datos", str(self.folder), *argv], out=out)
        return code, out.getvalue(), err.getvalue()
