"""Data for the hidden checks: a folder of their own, built with the
project's own loaders (only the public API the request names is used)."""
import contextlib
import csv
import io
import json
import shutil
import tempfile
import unittest
from datetime import datetime
from pathlib import Path

CLASSES = [
    {"id": "yoga", "name": "Yoga", "start": "2025-03-10T18:00", "capacity": 2, "instructor": "Marta", "room": "Sala 2", "minutes": 60},
    {"id": "spin", "name": "Spinning", "start": "2025-03-11T07:30", "capacity": 3, "instructor": "Raúl", "room": "Sala 1", "minutes": 45},
    {"id": "pila", "name": "Pilates", "start": "2025-03-11T19:00", "capacity": 5, "instructor": "Marta", "room": "Sala 2", "minutes": 60},
    {"id": "box", "name": "Boxeo", "start": "2025-03-12T20:00", "capacity": 5, "instructor": "Iván", "room": "Sala 1", "minutes": 90},
    {"id": "func", "name": "Funcional", "start": "2025-03-13T10:00", "capacity": 5, "instructor": "Iván", "room": "Sala 1", "minutes": 50},
    {"id": "yoga2", "name": "Yoga", "start": "2025-03-14T18:00", "capacity": 5, "instructor": "Marta", "room": "Sala 2", "minutes": 60},
    {"id": "early", "name": "Yoga", "start": "2025-03-03T18:00", "capacity": 5, "instructor": "Marta", "room": "Sala 2", "minutes": 60},
    {"id": "early2", "name": "Spinning", "start": "2025-03-05T18:00", "capacity": 5, "instructor": "Raúl", "room": "Sala 1", "minutes": 60},
    {"id": "func2", "name": "Funcional", "start": "2025-03-17T10:00", "capacity": 5, "instructor": "Iván", "room": "Sala 1", "minutes": 50},
    {"id": "func3", "name": "Funcional", "start": "2025-03-20T10:00", "capacity": 5, "instructor": "Iván", "room": "Sala 1", "minutes": 50},
    {"id": "abril", "name": "Yoga", "start": "2025-04-02T18:00", "capacity": 5, "instructor": "Marta", "room": "Sala 2", "minutes": 60},
]

MEMBERS = """id,name,email,plan,family
s001,Ana Pérez,ana@example.com,premium,perez
s002,Luis Gómez,luis@example.com,basic,
s003,Eva Ruiz,eva@example.com,basic,
s004,Tomás Díaz,tomas@example.com,premium,
s005,Bea Sanz,bea@example.com,pack,
s006,Hugo Pérez,hugo@example.com,basic,perez
s007,Rita Gil,rita@example.com,basic,
"""

# The same without the plan the request adds, for what worked before it.
MEMBERS_BEFORE = "".join(line + "\n" for line in MEMBERS.splitlines() if ",pack," not in line)

EARLY = datetime(2025, 3, 1, 9, 0)


def at(text):
    return datetime.fromisoformat(text)


class GymCase(unittest.TestCase):
    members = MEMBERS

    def setUp(self):
        from gym.app import Gym

        self.Gym = Gym
        self.folder = Path(tempfile.mkdtemp(prefix="gym-hidden-"))
        (self.folder / "classes.json").write_text(json.dumps(CLASSES), encoding="utf-8")
        (self.folder / "members.csv").write_text(self.members, encoding="utf-8")
        self.gym = Gym.load(self.folder)

    def tearDown(self):
        shutil.rmtree(self.folder, ignore_errors=True)

    def reload(self):
        self.gym.save()
        self.gym = self.Gym.load(self.folder)
        return self.gym

    def error(self, call, *args):
        from gym.models import GymError

        with self.assertRaises(GymError) as raised:
            call(*args)
        return str(raised.exception)

    def bodies(self, member):
        return [m.body for m in self.gym.outbox if m.to == member]

    def lines(self, member, month):
        return [tuple(line) for line in self.gym.invoice(member, month).lines]

    def cli(self, *argv):
        from gym.cli import main

        out = io.StringIO()
        try:
            with contextlib.redirect_stderr(io.StringIO()):
                code = main(["--datos", str(self.folder), *argv], out=out)
        except SystemExit as exit:
            code = exit.code
        return code, out.getvalue()

    def rows(self, text):
        return list(csv.reader(io.StringIO(text.strip())))
