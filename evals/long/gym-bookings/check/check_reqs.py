#!/usr/bin/env python3
"""Runs each requirement's hidden tests on its own and prints one line:

    REQS R01=1 R02=0 … R24=1 REG=1

Exit 0 only if all pass. `--changed`: the requirements the session-3 change
rewrites are checked as they are after it (test_hidden_changed.py).
"""
import io
import re
import sys
import unittest

sys.path.insert(0, ".")

import test_hidden_changed  # noqa: E402
import test_hidden_reqs  # noqa: E402

IDS = [f"R{n:02d}" for n in range(1, 25)] + ["REG"]
CHANGED = ["R03", "R05"]


def run(case):
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(case)
    result = unittest.TextTestRunner(stream=io.StringIO(), verbosity=0).run(suite)
    problems = result.failures + result.errors
    note = ""
    if problems:
        test, trace = problems[0]
        lines = [line for line in trace.strip().splitlines() if line.strip()]
        errors = [line for line in lines if re.match(r"^\w*(Error|Exception)\b", line)]
        last = (errors or lines)[-1]
        note = f"{test.id().split('.', 1)[1]}: {last[:160]}"
    return result.wasSuccessful() and result.testsRun > 0, note


def main():
    changed = "--changed" in sys.argv[1:]
    passed = {}
    # The tests' own noise (argparse, warnings) stays out of the output: the
    # REQS line has to be its end.
    sys.stderr = io.StringIO()
    for name in IDS:
        module = test_hidden_changed if changed and name in CHANGED else test_hidden_reqs
        passed[name], note = run(getattr(module, name))
        if note:
            print(f"✗ {note}")
    print("REQS " + " ".join(f"{name}={int(ok)}" for name, ok in passed.items()))
    return 0 if all(passed.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
