"""Ayudas de las pruebas ocultas: un taller nuevo, con los datos de partida, en una carpeta temporal."""
import contextlib
import io
import shutil
import tempfile
from datetime import date
from pathlib import Path

DATA = Path(__file__).resolve().parent / "hidden_data.json"
MONDAY = date(2025, 3, 10)
SATURDAY = date(2025, 3, 8)


def fresh():
    from taller.shop import Shop

    folder = Path(tempfile.mkdtemp())
    path = folder / "taller.json"
    shutil.copy(DATA, path)
    return Shop.load(path), path


def cli(path, *argv):
    from taller.cli import main

    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stderr(err):
        code = main(["--data", str(path), *argv], out=out)
    return code, out.getvalue(), err.getvalue()
