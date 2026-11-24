#!/usr/bin/env python3
"""Builds the confirmation test's cases from Commit0 (PLAN.md, «Cases»).

For each library of the pool, in the order the seed gives, it checks the rules
of eligibility and, while fewer than `--want` cases are eligible, writes one:

    evals/confirm/<lib>/case.json   the request, the change, the check
    evals/confirm/<lib>/repo/       Commit0's start (stubs), without its tests
    evals/confirm/<lib>/check/      the tests, the change's test and the grader

    python3 make_cases.py <bench> <venvs> <out> [--want 9] [--only lib,...]

<bench> holds Commit0's dataset rows (commit0_rows.json) and the clones
(repos/<lib>/src: branch commit0_combined; its parent is the reference).
<venvs> holds a Python environment per case (<venvs>/<lib>), with pytest and
the library's dependencies, never the library itself; the agent and the check
run with it. Prints what it found per library.
"""

import json, os, random, shutil, subprocess, sys, tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

SEED = 20261124
# Commit0's tasks (dataset commit0_combined) whose stubs remove 1 000–3 500 lines.
POOL = sorted("""tinydb voluptuous python-rsa cookiecutter pyjwt fabric imapclient minitorch
more-itertools pexpect marshmallow loguru bitstring attrs sqlparse requests graphene""".split())
# Rules 1–2, from each repository as published (PLAN.md).
EXCLUDED = {
    "minitorch": "regla 1: no tiene licencia en la raíz",
    "requests": "regla 2: sus tests llaman a un servicio en red (httpbin)",
    "pexpect": "regla 2: sus tests necesitan una terminal real (pty)",
}
# Development tools in the requirement files that no test needs.
TOOLING = ("sphinx", "coveralls", "tox", "mypy", "flake8", "black", "pre-commit", "ruff", "pytest-cov",
           "coverage", "pycodestyle", "pytest-pycodestyle", "types-", "isort", "autoflake", "codespell",
           "alabaster", "releases", "invocations", "twine", "build", "pip-tools", "bump", "towncrier",
           "furo", "myst", "pytest-xdist", "pytest-benchmark", "pytest-codspeed", "pytest-memray",
           "pylint", "pyright", "bandit", "check-manifest", "wheel", "setuptools", "pytest>", "pytest=",
           "pytest<", "pytest\n", "readme", "docutils", "nox", "taskipy")
MIN_PASS = 0.95
MIN_TESTS = 50
CHUNK = 10


def run(cmd, cwd=None, timeout=1800, env=None):
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout, env=env)
    return p.returncode, p.stdout + p.stderr


def tests_of(src: Path, row) -> list[Path]:
    """The test files of the start: its test folder, or the test files inside the package."""
    test_dir = src / row["test"]["test_dir"]
    if row["test"]["test_dir"].rstrip("/") == row["src_dir"].rstrip("/") or test_dir == src:
        found = []
        for p in test_dir.rglob("*.py"):
            if "__pycache__" in p.parts:
                continue
            rel = p.relative_to(src).parts
            if "tests" in rel or "test" in rel or p.name.startswith("test_") or p.name == "conftest.py":
                found.append(p)
        return found
    return [p for p in test_dir.rglob("*") if p.is_file() and "__pycache__" not in p.parts]


def package_of(row) -> str:
    name = Path(row["src_dir"].rstrip("/")).name
    # A src/ holding several packages: the one named as the library.
    return row["repo"].split("/")[1] if name == "src" else name


def python_path(row) -> str:
    """Where the package's folder is: the repo's root or its src/."""
    src = Path(row["src_dir"].rstrip("/"))
    if src.name == "src":
        return "src"
    return "." if str(src.parent) in ("", ".") else str(src.parent)


def install_deps(venv: Path, ref: Path, row) -> tuple[bool, str]:
    if not venv.exists():
        run([sys.executable, "-m", "venv", str(venv)])
    constraints = venv / "constraints.txt"
    constraints.write_text("pytest>=8.3,<8.4\n")
    pip = [str(venv / "bin" / "python"), "-m", "pip", "install", "-q", "-c", str(constraints)]
    code, out = run(pip + ["pytest<8.4", "pytest-timeout"])
    if code:
        return False, "no se pudo instalar pytest: " + out[-200:]
    wanted = [p for p in (row["setup"].get("pip_packages") or []) if not p.lower().startswith(TOOLING)]
    reqs = []
    for f in row["setup"].get("packages") or []:
        path = ref / f
        if path.exists():
            for line in path.read_text().splitlines():
                line = line.split("#")[0].strip()
                if line and not line.startswith("-") and not (line.lower() + "\n").startswith(TOOLING):
                    reqs.append(line)
    # Its own dependencies and its tests' extras, from the widest set to the
    # narrowest: with the development extra, without it, and only what the
    # dataset and its files list (also for metadata today's pip rejects).
    code, out = 1, ""
    for extras in ("[tests,test,testing,dev]", "[tests,test,testing]", None):
        what = ([f"{ref}{extras}"] if extras else []) + wanted + reqs
        if not what:
            code, out = 0, ""
            break
        code, out = run(pip + what, timeout=1800)
        if code == 0:
            break
    if code:
        return False, "regla 3: sus dependencias no se instalan: " + out.strip().splitlines()[-1][:200]
    # The library itself goes; only its dependencies stay. Every copy: one
    # installed by a dependency can sit beside the one from the reference.
    package = package_of(row)
    for _ in range(3):
        code, dist = run([str(venv / "bin" / "python"), "-c",
                          f"import importlib.metadata as m; print(' '.join(m.packages_distributions().get({package!r}, [])))"])
        if not dist.split():
            break
        for d in dist.split():
            run([str(venv / "bin" / "python"), "-m", "pip", "uninstall", "-q", "-y", d])
    code, _ = run([str(venv / "bin" / "python"), "-c", f"import {package}"], cwd="/")
    if code == 0:
        return False, f"regla 4: {package} sigue instalada como dependencia de otra cosa"
    return True, ""


def pytest_cmd(venv: Path, paths: list[str], xml: str, row) -> list[str]:
    return [str(venv / "bin" / "python"), "-m", "pytest", "-q", "-p", "no:cacheprovider", "-o", "addopts=",
            "-o", "junit_family=xunit1", "--timeout=60", "--continue-on-collection-errors",
            f"--junitxml={xml}", *paths]


def with_path(work: Path, row) -> dict:
    """The package's folder first on Python's path."""
    return {**os.environ, "PYTHONPATH": str((work / python_path(row)).resolve())}


def results(xml: str) -> dict[str, tuple[str, int, bool]]:
    """Each test (file::class::name) with its file, line and whether it passed."""
    out = {}
    if not os.path.exists(xml):
        return out
    for case in ET.parse(xml).getroot().iter("testcase"):
        file = case.get("file") or ""
        module = file[:-3].replace("/", ".") if file.endswith(".py") else ""
        cls = case.get("classname", "")
        cls = cls[len(module) + 1:] if module and cls.startswith(module + ".") else ""
        test = "::".join(x for x in (file, cls, case.get("name")) if x)
        failed = any(c.tag in ("failure", "error", "skipped") for c in case)
        out[test] = (file, int(case.get("line") or 0), not failed)
    return out


def groups(passing: dict[str, tuple[str, int, bool]]) -> dict[str, list[str]]:
    """The requirements: a test class each; a file's loose tests in groups of ten functions."""
    by_key: dict[tuple, list[str]] = {}
    loose: dict[str, list[tuple[int, str, str]]] = {}
    for test, (file, line, _) in passing.items():
        parts = test.split("::")
        if len(parts) >= 3:
            by_key.setdefault((file, parts[1]), []).append(test)
        else:
            base = parts[-1].split("[")[0]
            loose.setdefault(file, []).append((line, base, test))
    for file, tests in loose.items():
        bases = []
        for _, base, _ in sorted(tests):
            if base not in bases:
                bases.append(base)
        for line, base, test in tests:
            by_key.setdefault((file, f"#{bases.index(base) // CHUNK + 1}"), []).append(test)
    return {f"G{n:03d}": sorted(tests) for n, (_, tests) in enumerate(sorted(by_key.items()), 1)}


def build(lib: str, row, src: Path, ref: Path, venv: Path, out: Path, spec: dict) -> str:
    case = out / lib
    if case.exists():
        shutil.rmtree(case)
    tests = tests_of(src, row)
    # repo/: the start without its tests nor the docs' PDF (docs/ keeps their source).
    shutil.copytree(src, case / "repo", ignore=shutil.ignore_patterns(".git", "spec.pdf.bz2", "__pycache__", "*.afdesign"))
    for t in tests:
        (case / "repo" / t.relative_to(src)).unlink(missing_ok=True)
    check = case / "check"
    for t in tests:
        dest = check / t.relative_to(src)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(t, dest)
    # The reference's verdicts, to keep only what passes there.
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp) / "ref"
        shutil.copytree(ref, work, ignore=shutil.ignore_patterns(".git"))
        xml = str(Path(tmp) / "ref.xml")
        paths = sorted({str(t.relative_to(src).parts[0]) if row["test"]["test_dir"].rstrip("/") != row["src_dir"].rstrip("/")
                        else str(t.relative_to(src)) for t in tests if t.name.startswith("test")})
        code, log = run(pytest_cmd(venv, [row["test"]["test_dir"]] if row["test"]["test_dir"].rstrip("/") != row["src_dir"].rstrip("/") else paths, xml, row), cwd=work, timeout=1800, env=with_path(work, row))
        seen = results(xml)
        # The change's test against the reference with the change made by hand.
        change_ok, log2 = True, ""
        if spec.get("reference_change"):
            for rel, text in spec["reference_change"].items():
                (work / rel).parent.mkdir(parents=True, exist_ok=True)
                with open(work / rel, "a") as f:
                    f.write(text)
            shutil.copy2(Path(__file__).parent / "changes" / f"{lib}.py", work / "confirm_change_test.py")
            code2, log2 = run(pytest_cmd(venv, ["confirm_change_test.py"], str(Path(tmp) / "chg.xml"), row), cwd=work, env=with_path(work, row))
            change_ok = code2 == 0
    passing = {t: v for t, v in seen.items() if v[2]}
    if len(seen) < MIN_TESTS or len(passing) / max(len(seen), 1) < MIN_PASS:
        shutil.rmtree(case)
        return f"regla 5: la referencia pasa {len(passing)}/{len(seen)} tests aquí\n{log[-600:]}"
    grouped = groups(passing)
    (check / "confirm_expected.json").write_text(json.dumps(grouped, indent=1))
    shutil.copy2(Path(__file__).parent / "confirm_check.py", check / "confirm_check.py")
    if spec.get("change"):
        shutil.copy2(Path(__file__).parent / "changes" / f"{lib}.py", check / "confirm_change_test.py")
    package = package_of(row)
    case_json = {
        "title": f"Implementar {lib} desde sus stubs (Commit0), en varias sesiones",
        "prompt": PROMPT.format(lib=lib, package=package),
        "change": spec.get("change", ""),
        "check": f"python3 confirm_check.py {python_path(row)}",
        "tests": f"python3 -c \"import sys; sys.path.insert(0, '{python_path(row)}'); import {package}\"",
        "timeoutSecs": 3600,
    }
    (case / "case.json").write_text(json.dumps(case_json, indent=1, ensure_ascii=False) + "\n")
    note = "" if change_ok else " · ¡el test del cambio no pasa con la referencia cambiada!\n" + log2[-800:]
    return f"ok: {len(passing)}/{len(seen)} tests de la referencia pasan, {len(grouped)} requisitos{note}"


PROMPT = """Implementa la librería {lib} de este repositorio. En su código hay funciones y métodos cuyo cuerpo es solo su docstring y `pass`, y faltan funciones auxiliares que el resto del código usa: complétalo para que {package} funcione como la librería original, según sus docstrings y su documentación (docs/ y README).

El repositorio no trae los tests: se comprobará con la suite de tests de la librería, que no ves. Escribe tú el código: no instales ni copies la versión publicada de {lib}. Usa `python3`, que ya tiene pytest y las dependencias."""


def main():
    args = sys.argv[1:]
    want = 9
    only = None
    if "--want" in args:
        i = args.index("--want"); want = int(args[i + 1]); del args[i:i + 2]
    if "--only" in args:
        i = args.index("--only"); only = args[i + 1].split(","); del args[i:i + 2]
    bench, venv, out = Path(args[0]), Path(args[1]), Path(args[2])
    rows = {r["row"]["repo"].split("/")[1]: r["row"] for r in json.load(open(bench / "commit0_rows.json"))["rows"]}
    specs = json.load(open(Path(__file__).parent / "changes" / "changes.json")) if (Path(__file__).parent / "changes" / "changes.json").exists() else {}
    order = list(POOL)
    random.Random(SEED).shuffle(order)
    eligible = 0
    for lib in order:
        if eligible >= want:
            break
        if only and lib not in only:
            continue
        if lib in EXCLUDED:
            print(f"{lib}: {EXCLUDED[lib]}")
            continue
        row = rows[lib]
        src = bench / "repos" / lib / "src"
        if not src.exists():
            src.parent.mkdir(parents=True, exist_ok=True)
            code, log = run(["git", "clone", "-q", "--depth", "2", "-b", "commit0_combined",
                             f"https://github.com/{row['repo']}", str(src)])
            if code:
                print(f"{lib}: no se pudo clonar: {log[-200:]}")
                continue
        head = run(["git", "-C", str(src), "rev-parse", "HEAD"])[1].strip()
        if head != row["base_commit"]:
            print(f"{lib}: la rama no está en base_commit ({head})")
            continue
        ref = bench / "repos" / lib / "ref"
        if not ref.exists():
            run(["git", "-C", str(src), "worktree", "add", "-q", "--detach", str(ref), "HEAD~1"])
        ok, why = install_deps(venv / lib, ref, row)
        if not ok:
            print(f"{lib}: {why}")
            continue
        verdict = build(lib, row, src, ref, venv / lib, out, specs.get(lib, {}))
        print(f"{lib}: {verdict}", flush=True)
        if verdict.startswith("ok"):
            eligible += 1


if __name__ == "__main__":
    main()
