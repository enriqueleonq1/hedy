# Confirmation test: results (Nov 24, 2026)

Run as `PLAN.md` and its six amendments fixed it:

- eight Commit0 libraries;
- each session a new process, cut at 600 000 tokens at input price;
- the change asked with session 2;
- up to six sessions per run;
- haiku, one run per case and arm.

hedy-hoy and documento ran in all eight cases. pedido-repetido and capa-de-terceros stopped where
amendment 6 left them. The flow did not run.

    python3 analyze.py ../../confirm data/haiku.jsonl --costs data/costs.json

`data/` holds:

- `haiku.jsonl`: every main session, the voided ones included;
- `pilot.jsonl`: the pilots';
- `costs.json`: each run priced from its logs by `price.py`;
- `diffs/`: each run's final patch.

In `data/`, paths are shortened and the layer's own name and its registry files are left out.

## Cost and runs

| | USD |
|---|---|
| Sonnet pilot (3 runs + 1 voided) | 10.54 |
| haiku pilot (3 runs + 4 voided) | 6.29 |
| haiku main runs used (19) | 14.08 |
| haiku main runs voided at 13:14 by an account error (403), still in the data (8) | 3.66 |
| haiku runs cut when the session driving them closed (8, no rows) | 4.77 |
| **total, from the session logs at list prices** | **39.34** of 60 |

Runs voided and run again:

- the pilots: 3 by the personal account's usage limit, 1 by a shared Hedy database;
- the main runs: 8 by the 403, of which 5 were run again to completion;
- 8 more were cut by the closing (amendment 5).

None is counted twice.

## haiku (8 cases, 1 run per case and arm)

Requirements met after the last session, mean over cases. Interval: paired bootstrap over
cases, 10 000 resamples, seed 7.

| arm | runs | requirements | Δ vs hedy-hoy | 95 % | change respected | regressions / run | sessions | $ / run | × hedy-hoy | hidden tests passed |
|---|---|---|---|---|---|---|---|---|---|---|
| hedy-hoy | 8 | 60 % | | | 38 % (3/8) | 0.12 | 4.5 | 0.740 | 1.00 | 76 % |
| **documento** | 8 | 56 % | **−4** | **[−7, −1]** | **88 % (7/8)** | 0.12 | 3.9 | 0.614 | 0.83 | 76 % |
| pedido-repetido | 1 | 82 % | +7 (marshmallow only) | | 1/1 | 0.00 | 4.0 | 0.870 | | 91 % |
| capa-de-terceros | 2 | 54 % | −3 | [−9, +3] | 1/2 | 6.50 | 5.5 | 1.193 | 1.61 | 77 % |

The change, paired by case, documento − hedy-hoy: **+50 points [+13, +88]**.

No run finished every requirement. No case was left out by the plan's rule: every first
session was cut.

By case (requirements met after each session; ✓ = change met at the end):

| case | hedy-hoy | documento | other arms |
|---|---|---|---|
| attrs (85) | 17→51→58→58 | 18→57→57 ✓ | layer 0→13→0→0→42→50 |
| cookiecutter (64) | 3→3→4→21→29→29 | 3→3→3→5→11→24 ✓ | layer 3→18→18→25→31 ✓ |
| marshmallow (68) | 0→0→11→51 ✓ | 0→0→0→0→51 ✓ | request 0→0→56→56 ✓ |
| more-itertools (149) | 0 in all 6 | 0 in all 6 | |
| python-rsa (27) | 23→23 ✓ | 7→23 ✓ | |
| sqlparse (55) | 1→33→33→35→38→38 | 0→2→31→33 ✓ | |
| tinydb (17) | 4→13 ✓ | 10→11 ✓ | |
| voluptuous (15) | 0→0→0→2→9→9 | 0→2→9 ✓ | |

## What the runs show

- **Requirements.** The document never came out ahead of hedy-hoy:
  - it tied in four cases and was behind in four, by 1–12 points;
  - it ended by itself sooner (3.9 sessions against 4.5): in sqlparse at 33 of 55 while
    hedy-hoy went on to 38, and in attrs one session sooner at the same level.

  Unlike test C, the task does not get lost here: hedy-hoy carries the request as written,
  so a new session knows what to do. That was the solo arm's failure in C, the one the
  document fixed.
- **The change.** Here the difference is large.
  - hedy-hoy lost it in every run where session 2 was cut before it was done (attrs,
    cookiecutter, sqlparse, voluptuous). From session 3 on, its message no longer has it:
    Hedy's handoff carries the original request but not what the user said after it.
  - The document did it in session 2 or 3 in every case that imports.
- **more-itertools** scores 0 for both arms: neither added the helper `raise_`, which the
  stubs removed and `__all__` still names, so the hidden suite does not import. The request
  warns that helpers are missing. The case stays as fixed, but it weighs nothing in the
  difference.
- **The layer** ran in two cases. It met about as much as hedy-hoy, at 1.6× the cost, with
  13 regressions in attrs: its second session met 13 requirements and its third undid them.
  In cookiecutter it kept the change, in attrs it did not.

## Rules (PLAN.md, applied to haiku as amendment 1 says)

| rule | result | verdict |
|---|---|---|
| documento beats hedy-hoy by ≥ +10, lower bound > 0, cost ≤ 1.2×, regressions ≤ | −4 [−7, −1], 0.83×, regressions equal | **not confirmed** |
| hedy-hoy within 5 points of documento | hedy-hoy is 4 points ahead | **the document is not a priority** |
| documento ≈ pedido-repetido | pedido-repetido ran in one case | not answered (amendment 6) |
| layer or flow beats documento by ≥ +10 | layer +1 [−8, +11] in 2 cases; flow not run | not answered: too few runs, no flow |

## Limits

- One run per case and arm, eight cases, haiku only. The interval is over cases, not over
  the noise of a single run.
- The cut is a token budget standing in for a usage limit; with these libraries almost every
  session of every arm was cut. The Sonnet pilot met 3–20 % of imapclient in six sessions, so
  Sonnet's numbers would not be these.
- The change is a single small function asked in session 2. That it is lost is what
  hedy-hoy's message does, not what the agent chooses. A later change, or several, would
  weigh more.
- Runs 8 at a time on one account met one wave of 403 errors (13:14); it did not recur.
