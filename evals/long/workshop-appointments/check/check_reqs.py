"""Corre la prueba oculta de cada requisito por separado y resume el resultado.

    python3 check_reqs.py [--changed]

Imprime al final una línea `REQS R01=1 R02=0 … REG=1` y sale con 0 solo si todo pasa.
Con --changed, los requisitos que cambiaron en la última sesión se prueban con lo nuevo.
"""
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(Path.cwd()))

IDS = [f"R{n:02d}" for n in range(1, 25)]
CHANGED = {"R03", "R13", "R24"}


def run(name: str) -> bool:
    try:
        import test_hidden_reqs as module

        suite = unittest.defaultTestLoader.loadTestsFromName(name, module)
    except Exception as error:  # el código no se puede importar
        print(f"{name}: {error!r}", file=sys.stderr)
        return False
    result = unittest.TextTestRunner(stream=sys.stderr, verbosity=0).run(suite)
    return result.wasSuccessful() and result.testsRun > 0


def main() -> int:
    changed = "--changed" in sys.argv[1:]
    results = {}
    for req in IDS:
        name = f"{req}Changed" if changed and req in CHANGED else req
        results[req] = run(name)
    results["REG"] = run("REG")
    print("REQS " + " ".join(f"{k}={int(v)}" for k, v in results.items()))
    return 0 if all(results.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
