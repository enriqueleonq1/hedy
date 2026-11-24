# Confirmation cases (Commit0)

Eight libraries and a pilot, taken from Commit0 (github.com/commit-0/commit0, MIT; dataset
`wentingzhao/commit0_combined`) by the rule in `evals/experiments/confirmation/PLAN.md` and
built with `make_cases.py` there. Each `repo/` is Commit0's start for that library (functions
emptied to their docstring), with the library's own license file; `check/` holds the library's
tests, which the agent never sees, and the grader. The code belongs to each library's authors
under its license (MIT, BSD or Apache-2.0).
