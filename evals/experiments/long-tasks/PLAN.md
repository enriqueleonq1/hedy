# Test C: what helps a long task across sessions?

Written before any run of this test (Nov 23, 2026). The cap of actions is the one number
set after a calibration run (below); nothing else changes after the data.

**Question.** When a task does not fit in one session and every session starts from
nothing but the code, what keeps the work on track: repeating the request, a task document
Hedy keeps, a configuration layer with its own memory, or a flow?

## Cases

Three cases in `evals/long/` (`gym-bookings`, `coworking-invoices`,
`workshop-appointments`), each an existing project of about 1 200 lines in 16–17 modules and
24 numbered requirements in the user's words that touch at least 6 of them (the reference
changes 12–14 files, about 600 lines), with:

- a hidden test per requirement and a regression test of what the starting repo already did
  (`check_reqs.py` prints `REQS R01=1 … REG=1`);
- at least three requirements with exact texts (`exact`), two that depend on each other
  (`linked`), and a change asked in the last session that rewrites one or two of them
  (`changed`, checked with `checkChanged`);
- a reference solution that passes all of them (`reference/`), and one with the change
  (`reference-changed/`); the start passes none.

## Sessions

Three sessions per run (`hedy-evals --long`), each a new task: a new agent process, no
resume, no memory shared but the code. What a session leaves is committed before the next.
Each session is cut after a cap of actions (`--cap`). Cases run on the `plain` layer (no
Hedy memory: nothing shared between sessions but the code).

The user's messages are the same in every arm:

1. «Implementa esto: <the 24 requirements>»
2. «Continúa.»
3. «Cambia esto y termina: <the change>»

## Arms

- **solo**: one AI, nothing added.
- **repeat**: one AI, and the request repeated after the message in sessions 2 and 3.
- **doc**: one AI, and after the message a task document Hedy keeps: the request as written
  (never summarised), the change once asked, and each session's progress (files changed,
  whether the project's tests pass, the AI's last message).
- **layer**: one AI with the third-party configuration layer of docs/decisions.md §50.6
  installed in the test account (its own memory across sessions included).
- **flow**: each session runs as the `essential` flow (tests first, the tests decide), under
  the same cap, which counts every role's actions.

## Runs

- Haiku: every arm, 3 repeats per case (45 runs, 135 sessions), cap 20.
- Sonnet: 2 repeats per case on `solo` and the arms that separate from it with haiku (a
  difference of at least 15 points in requirements met after session 3).
- Opus: 1 run per case, the best arm against `solo`.

## Measures (per session)

- Requirements met (of 24), and the regression test.
- Regressions: a requirement met after one session and not after the next (the changed ones
  at session 3 are judged with the changed check).
- Exact texts: the `exact` requirements met.
- The change: the `changed` requirements met under `checkChanged` after session 3.
- The linked pair: both met.
- Files read before the first edit in sessions 2 and 3 (how long it takes to find its way
  again).
- Tokens and cost; cost per requirement met.
- Uncertainty: 95 % intervals by bootstrap over cases, paired with `solo` by case and repeat.
  With three cases the intervals will be wide: only large differences will show.

## Decision (fixed now)

- Build **the task document** if `doc` beats `solo` by at least 15 points in requirements met
  after session 3 (haiku, mean over cases), with no more regressions, at no more than 1.3×
  the cost. If `repeat` does as well (within 5 points), repeating the request is enough and is
  what gets built (it is simpler); the document then has to earn its place by the change or
  the regressions.
- If `flow` beats no other arm on requirements met, regressions or cost per requirement, flows
  stay an advanced option.
- `layer` is a reference point, not a candidate.

## Calibration (before the runs, Nov 23, 2026)

A first version of the cases (12 requirements, one new module) was solved by haiku in its
first session with one or two edits, cap or not: it was replaced by the larger one above.
On the larger cases, `solo` with haiku, one run per case and cap:

| cap | coworking (S1 → S3) | gym (S1 → S3) | workshop (S1 → S3) |
|---|---|---|---|
| 10 | 0 → 0 | 0 → 1 | 0 → 0 |
| 12 | 1 → 2 | 1 → 3 | 23 → 23 |
| 15 | 0 → 2 | 0 → 1 | 0 → 0 |
| 20 | 24 → 24 | | |

Haiku either writes nearly everything at once or explores and gets little done; no cap gives
«about half» steadily. **Cap: 20**, where a session sometimes finishes and sometimes is cut.
With `solo`, a session 2 that only hears «Continúa.» sometimes ends without an edit.

Runs spend the account of the session that drives them (`~/.claude`); the test accounts link
to it, the layer's through a link to its credentials file.
