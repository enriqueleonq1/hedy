# Test A: run the project's tests after the agent and send the failure back

Written before any run of this test (Nov 23, 2026). Nothing here changes after the data.

**Question.** When Hedy runs the project's tests after the agent and sends a failure back,
does the result get better, at a cost paid only when it fires?

## Setup

- Runner: `hedy-evals --rescue --diffs --layers memory --model haiku --repeat 3`.
- Cases (13): the cases with a code change and their own tests (`tests` in `case.json`)
  where earlier haiku runs failed at least once (`attachment-download`, `billing-cents`,
  `business-days`, `inventory-expiry`, `invoice-rounding`, `library-holiday-fine`,
  `parse-duration`, `rate-limit-bugs`, `receipt-api-key`, `sales-summary-rules`,
  `shipping-refactor`), plus two that never failed, to see breakage (`bakery-inscription`,
  `easy-greeting`). Left out: `price-format` and `price-format-team` (no tests of their own,
  so nothing can fire), `rename-field` (the request edits the tests), `library-holds-question`
  (no code).
- Before: `stability.py` ran each case's tests and hidden check three times on the start and
  on a reference solution (`reference/`). All 13 were stable: the hidden check fails on the
  start and both pass on the reference. The project's tests already pass on the start in
  12 of 13 cases (not in `invoice-rounding`).

## Arms

Each run has one first attempt, shared by the four arms, so they pair:

- **base**: the first attempt (one AI, as today).
- **same**: if the project's tests fail after the first attempt, their output goes back to
  the same session once: «Corrí los tests del proyecto (`<cmd>`) y fallan: <output> Corrígelo.»
- **fresh**: the same, in a new session that starts from the first attempt's code (committed
  as «Intento anterior»), with the request, then the failure.
- **retry** (control at the same spend): a second attempt from the start, without feedback,
  delivered only if it passes the project's tests (else the first attempt stands).

When the tests pass after the first attempt, all four arms are that attempt. The feedback is
at most 2 000 characters of the end of the test output.

## Measures

- Hidden check passed, per arm.
- Paired with base, per run: **rescues** (base fails, arm passes), **breaks** (base passes,
  arm fails), extra tokens and cost, and tokens per rescue.
- How often the trigger fires (tests fail after the first attempt).
- **Invalid**: an arm whose delivered code removes or changes lines of the project's own test
  files (added lines, changed imports and blank lines are fine). It is excluded from that
  arm's comparison, and counted.
- A run whose first attempt ends in an error (timeout, limit, 0 tokens) is excluded from all
  arms and run again.
- Uncertainty: 95 % interval of each arm's pass-rate difference with base, by bootstrap over
  cases (10 000 resamples of the 13 cases, runs kept with their case).

## Decision (fixed now)

Build «Verificar y devolver el fallo» if `same` or `fresh`:

1. rescues at least 2 runs, in at least 2 cases;
2. breaks none;
3. rescues more runs than `retry` (feedback, not just a second try, is what helps);
4. adds no tokens to runs where the tests passed (true by construction; checked).

If the trigger fires in fewer than 5 % of first attempts, the feature cannot pay on this
suite whatever the arms do: no default, and the note that the project's tests are the
bottleneck (as `escalar` found in docs/decisions.md §50).

Sonnet runs only if haiku leaves the decision open: an arm meets 1–3 but the interval of its
difference includes zero. Then Sonnet, 3 repeats, on the cases where haiku's tests fired.

## Prediction (also written before)

Few triggers: the project's tests pass on the start in 12 of 13 cases, and earlier haiku
failures were mostly in what those tests do not cover.
