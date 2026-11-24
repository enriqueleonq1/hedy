"""Una carpeta de datos de prueba, con pocas clases y socios."""
import json
import shutil
import tempfile
from pathlib import Path

CLASSES = [
    {"id": "yoga", "name": "Yoga", "start": "2025-03-10T18:00", "capacity": 2, "instructor": "Marta", "room": "Sala 2", "minutes": 60},
    {"id": "spin", "name": "Spinning", "start": "2025-03-11T07:30", "capacity": 3, "instructor": "Raúl", "room": "Sala 1", "minutes": 45},
]

MEMBERS = """id,name,email,plan,family
s001,Ana Pérez,ana@example.com,premium,perez
s002,Luis Gómez,luis@example.com,basic,
s003,Eva Ruiz,eva@example.com,basic,
"""


def make_folder(classes=CLASSES, members=MEMBERS):
    folder = Path(tempfile.mkdtemp(prefix="gym-test-"))
    (folder / "classes.json").write_text(json.dumps(classes), encoding="utf-8")
    (folder / "members.csv").write_text(members, encoding="utf-8")
    return folder


def remove(folder):
    shutil.rmtree(folder, ignore_errors=True)
