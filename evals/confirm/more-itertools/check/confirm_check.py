#!/usr/bin/env python3
"""The confirmation test's hidden check (copied into the work as it ends).

Runs the library's tests with pytest and prints, last:

    TESTS <passed>/<tests>
    REQS G001=1 G002=0 … CHG=1

A requirement (G…) is met when every test of its group passes; the groups and
their tests are those that pass with the reference (confirm_expected.json).
CHG is the change asked mid-task (confirm_change_test.py).

    python3 confirm_check.py <folder the package is in>
"""

import json, os, subprocess, sys, tempfile
import xml.etree.ElementTree as ET


def verdicts(xml):
    out = {}
    if not os.path.exists(xml):
        return out
    for case in ET.parse(xml).getroot().iter("testcase"):
        file = case.get("file") or ""
        module = file[:-3].replace("/", ".") if file.endswith(".py") else ""
        cls = case.get("classname", "")
        cls = cls[len(module) + 1:] if module and cls.startswith(module + ".") else ""
        test = "::".join(x for x in (file, cls, case.get("name")) if x)
        out[test] = not any(c.tag in ("failure", "error", "skipped") for c in case)
    return out


def pytest(paths, xml, path):
    cmd = [sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", "-o", "addopts=",
           "-o", "junit_family=xunit1", "--timeout=20", "--continue-on-collection-errors",
           f"--junitxml={xml}", *paths]
    env = {**os.environ, "PYTHONPATH": os.path.abspath(path)}
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=780, env=env)
        return p.stdout[-1500:] + p.stderr[-500:]
    except subprocess.TimeoutExpired:
        return "pytest no terminó en 13 min"


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "."
    expected = json.load(open("confirm_expected.json"))
    files = sorted({t.split("::")[0] for tests in expected.values() for t in tests})
    with tempfile.TemporaryDirectory() as tmp:
        log = pytest(files, os.path.join(tmp, "tests.xml"), path)
        seen = verdicts(os.path.join(tmp, "tests.xml"))
        change = None
        if os.path.exists("confirm_change_test.py"):
            pytest(["confirm_change_test.py"], os.path.join(tmp, "change.xml"), path)
            got = verdicts(os.path.join(tmp, "change.xml"))
            change = bool(got) and all(got.values())
    print(log, file=sys.stderr)
    tests = [t for group in expected.values() for t in group]
    print(f"TESTS {sum(seen.get(t, False) for t in tests)}/{len(tests)}")
    reqs = [f"{g}={int(all(seen.get(t, False) for t in group))}" for g, group in expected.items()]
    if change is not None:
        reqs.append(f"CHG={int(change)}")
    print("REQS " + " ".join(reqs))
    sys.exit(0 if all(r.endswith("=1") for r in reqs) else 1)


if __name__ == "__main__":
    main()
