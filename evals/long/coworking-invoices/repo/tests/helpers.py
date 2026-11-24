import shutil
import tempfile
from pathlib import Path

from cowork.storage import Store

DATA = Path(__file__).resolve().parent.parent / "data"


def fresh_store(test):
    """Una copia de data/ en una carpeta temporal, cargada."""
    folder = Path(tempfile.mkdtemp())
    test.addCleanup(shutil.rmtree, folder, True)
    for item in DATA.iterdir():
        shutil.copy(item, folder / item.name)
    return Store.load(folder)
