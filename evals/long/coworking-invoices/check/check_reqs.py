"""Runs each requirement's hidden checks on its own and prints one line:
REQS R01=1 R02=0 ... REG=1. Exits 0 only if all pass. --changed checks the
requirements as they stand after the change of the third session."""
import io
import sys
import unittest

IDS = [f"R{n:02d}" for n in range(1, 25)] + ["REG"]
CHANGED = {"R01", "R02"}


def main():
    changed = "--changed" in sys.argv
    sys.path.insert(0, ".")
    import test_hidden_reqs as hidden

    results, notes = {}, []
    for rid in IDS:
        cls = getattr(hidden, f"{rid}Changed" if changed and rid in CHANGED else rid)
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(cls)
        stream = io.StringIO()
        outcome = unittest.TextTestRunner(stream=stream, verbosity=0).run(suite)
        results[rid] = int(outcome.wasSuccessful())
        if not outcome.wasSuccessful():
            problems = outcome.failures + outcome.errors
            last = problems[0][1].strip().splitlines()[-1] if problems else "?"
            notes.append(f"{rid}: {last[:200]}")
    for note in notes:
        print(note)
    print("REQS " + " ".join(f"{rid}={ok}" for rid, ok in results.items()))
    return 0 if all(results.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
