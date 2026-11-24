# Test D: results (Nov 24, 2026)

Run as `PLAN.md` fixed it: five cases × four arms × three repeats, `claude-haiku-5-5`, account
`claude-personal`, layer `plain`. 60 runs, none voided, no errors. About $0.63 by the CLI's
estimate.

    python3 analyze.py data/haiku.jsonl

`data/haiku.jsonl` holds every run; `data/diffs/` each run's final patch with the hidden
check's verdict beside it (temporary paths shortened).

## By case (passed / runs, mean $ per run)

| case | solo | informed | plan | approval |
|---|---|---|---|---|
| attachment-download | 3/3 · 0.005 | 3/3 · 0.006 | 3/3 · 0.012 | 2/3 · 0.018 |
| billing-cents | 3/3 · 0.011 | 3/3 · 0.009 | 3/3 · 0.016 | 3/3 · 0.025 |
| inventory-expiry | 1/3 · 0.008 | 3/3 · 0.007 | 1/3 · 0.014 | 1/3 · 0.018 |
| price-format | 0/3 · 0.004 | 3/3 · 0.003 | 3/3 · 0.007 | 3/3 · 0.017 |
| sales-summary-rules | 0/3 · 0.004 | 3/3 · 0.004 | 3/3 · 0.009 | 3/3 · 0.013 |

## All cases

| arm | passed | $ / run | × plan | seconds | corrected |
|---|---|---|---|---|---|
| solo | 47 % | 0.006 | 0.55 | 29 | |
| informed | 100 % | 0.006 | 0.49 | 24 | |
| plan | 87 % | 0.012 | 1.00 | 47 | 15/15 |
| approval | 80 % | 0.018 | 1.58 | 80 | 15/15 |

Paired by case (bootstrap over cases, 10 000 resamples, seed 7):

| difference | passed (points) | 95 % | cost ratio |
|---|---|---|---|
| plan − approval | +7 | [+0, +20] | 0.63× |
| plan − informed | −13 | [−40, +0] | 2.03× |
| plan − solo | +40 | [+0, +80] | 1.83× |
| approval − solo | +33 | [−13, +80] | 2.91× |

## What the failures were

Read in each failing run's verdict:

- **price-format, sales-summary-rules (solo 0/6).** Exactly what the request left out:
  `'$12.50' != 'USD 12.50'`; `print` instead of logging. Every arm that heard the
  clarification passed both (18/18).
- **attachment-download, billing-cents.** Haiku guarded the path and rounded half up without
  being told (solo 6/6). The one failure, `approval` in attachment-download, returned the
  file as bytes instead of text; the path checks passed.
- **inventory-expiry.** Every failure, in every arm, is the same hidden test:
  - after `release_expired()`, `reservations()` still lists the released ones (`3 != 1`);
  - neither the request nor the clarification says it;
  - the 15-minute boundary and the confirmed reservations were right in every run.

  `informed` passed it 3/3 and the other arms 1/3 each; with three runs that may be noise.
- **The clarification was never lost.** It reached the agent in all 30 checkpoint runs, and no
  failure is about what it said, in the same session or after the handoff to a new one.

## Rules (PLAN.md)

| rule | result | verdict |
|---|---|---|
| Part 1: `plan` within 10 points of `informed` | −13 [−40, 0] | **not met** as written. The whole gap is inventory-expiry, whose failures are not about the clarification. Where the request really left something out (price-format, sales-summary-rules), `plan` 6/6 = `informed` 6/6, against `solo` 0/6. |
| Part 2: `plan` ≥ `approval` − 5 and cost ≤ 0.8× | +7 [0, +20], 0.63× | **met**: the user's way of working is one AI with checkpoints (the same session) plus checks Hedy runs; separate sessions are not needed for it |
| Part 2: `approval` beats `plan` by ≥ 10 | −7 | no |

## Found on the way

Claude Code (2.1.306) runs another model in plan mode when the session's model is the alias
`haiku` (or `opusplan`). A probe's plan-mode session ran entirely on `claude-sonnet-5-5`, and
the pilot's `plan` cost 7–14× the other arms. With the full id the session stays on haiku.
Hedy's «Enséñame el plan primero» with haiku therefore spends Sonnet while it shows haiku.

## Limits

- Five small cases with one gap each, haiku only, three repeats; the interval is over cases.
- The simulated user always says the same thing at the first checkpoint. The test measures
  what happens to a correction once given, not whether a person would spot the gap in a plan.
- In two of the five cases haiku did not need the clarification, so only two separate `solo`
  from the rest.
- `approval` also writes the tests first and runs them as a gate. The difference against
  `plan` is that whole flow, not only the separate sessions.
