import shutil
import tempfile
from pathlib import Path

from taller.shop import Shop
from taller.storage import DEFAULT_PATH


def fresh_shop() -> tuple[Shop, Path]:
    """Una copia de los datos de ejemplo en una carpeta temporal, y el taller cargado de ella."""
    folder = Path(tempfile.mkdtemp())
    path = folder / "taller.json"
    shutil.copy(DEFAULT_PATH, path)
    return Shop.load(path), path
