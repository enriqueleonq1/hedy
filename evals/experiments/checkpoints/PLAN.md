# Test D: one AI with checkpoints, or a flow with separate sessions?

Written before the main runs (Nov 24, 2026). The pilot (below) only checked that the runner
works; its runs are not in the analysis.

**Question.** A user's own way of working is often: analyse the request, plan, have the plan
checked, implement, check the result. Does that need several AIs (a flow), or is it one AI
that stops for the user's word? Two parts:

1. **Does the checkpoint bring in what only the user knows?** Asked as a request that leaves
   something out, does stopping at the plan and hearing the user's correction get as far as
   asking the whole thing from the start?
2. **Does it matter whether the session that planned goes on to implement?** Hedy has both
   today: «Enséñame el plan primero» (one AI: the same session plans, hears the user and
   implements) and the «Con tu aprobación» flow (an architect plans in its session; a
   programmer implements in a new one, with the plan, the user's feedback and the tests as
   the gate).

## Cases

Five existing cases (`evals/`), each with a `checkpoint` in `case.json`: the request as asked
here (by default the case's own) and what the user adds at the plan (`clarify`). The hidden
check is the case's own and tests what the clarification says.

| case | what the request leaves out |
|---|---|
| `price-format` | the display format («USD 12.50») and that amounts are integer cents |
| `sales-summary-rules` | money in `Decimal`; `reports/` logs, never prints (the request says «show it in the console») |
| `attachment-download` | the name must not leave the uploads folder (`..`, absolute, `%2e`/`%2f`) |
| `inventory-expiry` | at exactly 15 minutes it has expired; confirmed ones never expire |
| `billing-cents` | every rounding is to the cent, half up |

Layer `plain` (no Hedy memory: in `price-format` and `sales-summary-rules` what the user adds
is what the memory notes would say).

## Arms (`hedy-evals --checkpoints`)

- **solo**: one AI with the request, as Hedy works by default.
- **informed**: one AI with the request and the clarification together. The reference: what
  the user would get by writing everything up front.
- **plan**: one AI with «Enséñame el plan primero» (Claude Code's plan mode). The user
  answers the first plan with the clarification (sent back «El usuario pide cambios al plan:
  …»), approves the next, and the same session implements.
- **approval**: the «Con tu aprobación» flow: architect → approval → programmer that writes
  the tests first → project tests (back to the programmer on failure). The user answers the
  first approval with the clarification (back to the architect), approves the next.

The simulated user always says the same text at the first checkpoint, whatever the plan
says. It measures what happens to a correction once given, not whether a person would spot
the gap.

## Runs

Haiku as `--model claude-haiku-5-5` (the full id: see the pilot), account `claude-personal`,
5 cases × 4 arms × 3 repeats = 60 runs, `--diffs`. The arms of a case and repeat run back to
back.

## Measures

- Passed: the case's hidden check (and the project's tests).
- Cost: the CLI's estimate (`costUsd`), mean per run; seconds.
- Corrected: the clarification reached the agent (the checkpoint happened).
- Intervals: paired bootstrap over cases (mean of the per-case differences in pass rate),
  10 000 resamples, seed 7. With five cases only large differences show.
- Also reported over the cases where `solo` passes in under half its runs (where the request
  really leaves something out).

## Decision (fixed now)

- **Part 1.** If `plan` is within 10 points of `informed` in pass rate, the checkpoint brings
  in what the user knows. Its cost against `informed` is reported.
- **Part 2.**
  - If `plan` is no more than 5 points below `approval` and costs at most 0.8× as much: the
    user's way of working is built as one AI with checkpoints (the same session) plus checks
    Hedy runs; separate sessions are not needed for it.
  - If `approval` beats `plan` by 10 points or more, with the lower bound above 0: separate
    sessions with the tests as the gate pay; that way of working stays a flow.
  - Otherwise: no difference shown; the cheaper one is the default for it.
- A run where the checkpoint never happened (`corrected` false in `plan` or `approval`) counts
  as it came out, and the count is reported.

## Pilot (before this plan)

One run of each arm on `price-format` and `inventory-expiry`, to check the runner: that the
plan and the approval are answered with the clarification once and then approved, and that
the runs finish. They did (2 checkpoints each, corrected), but `plan` cost 7–14× the other
arms on fewer tokens.

The cause: Claude Code (2.1.306) runs another model in plan mode when the session's model is
the alias `haiku` (or `opusplan`); in a probe its plan-mode session ran entirely on
`claude-sonnet-5-5`. With the full id, `claude-haiku-5-5`, plan mode stays on haiku (probe and
a rerun of `plan` on `price-format`: $0.007, 2 checkpoints, passed). So every arm runs with the
full id, and the pilot's runs with the alias are left out.
