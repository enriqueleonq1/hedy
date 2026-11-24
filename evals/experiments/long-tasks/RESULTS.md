# Test C: results (Nov 24, 2026)

Run as `PLAN.md` fixed it: three cases, three fresh sessions per run, cap 20 actions per
session. Haiku on every arm (45 runs, 135 sessions); Sonnet on `solo`, `repeat` and `doc` (18
runs); Opus on `solo` and `doc` (6 runs). One more batch after the plan's, marked as such:
Sonnet with a cap of 5. Runs a usage limit voided were run again and left out
(`analyze.py` keeps runs with three sessions, no error and tokens spent).

    python3 analyze.py ../../long data/haiku-cap20.jsonl --layer-account layer

Cost (the CLI's estimates; a session cut at its cap is priced at its model's median rate):
haiku $4.92, Sonnet $13.31, Opus $17.33, Sonnet cap 5 $5.72, voided runs included.

## Haiku, cap 20 (3 cases × 3 repeats per arm)

Requirements met of 24, mean over runs; interval of the difference with `solo` after session
3 by bootstrap over cases.

| arm | S1 | S2 | S3 | Δ S3 vs solo | 95 % | regressions | exact texts | the change | linked pair | $ / run | $ / req. |
|---|---|---|---|---|---|---|---|---|---|---|---|
| solo | 57 % | 60 % | 61 % | | | 0.0 | 62 % | 78 % | 44 % | 0.107 | 0.0073 |
| repeat | 56 % | 76 % | 90 % | +29 | [+24, +33] | 0.0 | 92 % | 89 % | 67 % | 0.114 | 0.0052 |
| **doc** | 50 % | **99 %** | **99 %** | **+38** | **[+26, +53]** | 0.0 | 100 % | 100 % | 67 % | 0.115 | 0.0049 |
| layer | 20 % | 46 % | 47 % | −14 | [−36, +28] | 0.0 | 48 % | 61 % | 22 % | 0.089 | 0.0079 |
| flow | 38 % | 62 % | 75 % | +13 | [−6, +29] | 0.2 | 79 % | 70 % | 67 % | 0.122 | 0.0068 |

The regression test of what the starting repo did passed in every run.

By case (requirements met after each session, each run):

| case | solo | repeat | doc | layer | flow |
|---|---|---|---|---|---|
| coworking-invoices | 3→4→5, 24→24→24, 24→24→24 | 6→24→24, 23→23→23, 24→24→23 | 0→24→24, 24→24→24, 4→24→24 | 24→24→24, 0→0→1, 0→1→2 | 0→6→19, 6→6→23, 6→23→23 |
| gym-bookings | 0→0→1, 23→23→23, 1→7→7 | 22→22→22, 0→0→23, 0→0→8 | 1→23→23, 23→23→23, 8→23→23 | 0→8→8, 19→22→22, 0→21→21 | 0→18→18, 0→7→7, 0→2→2 |
| workshop-appointments | 24→24→24, 24→24→24, 0→0→0 | 23→24→24, 24→24→24, 0→24→24 | 24→24→24, 0→24→24, 24→24→24 | 0→0→0, 0→24→24, 0→0→0 | 23→23→23, 24→24→24, 24→24→24 |

Haiku either writes nearly everything in its first session or is cut while exploring, so
what decides an arm is what happens to the runs cut early. Of those (under 20 of 24 after
session 1), how many got to 20 by session 3 (descriptive, not in the plan):

| solo | repeat | doc | layer | flow |
|---|---|---|---|---|
| 0/4 | 3/4 | **5/5** | 3/8 | 2/6 |

- **Why solo stalls.** A new session that only hears «Continúa.» does not know what the task
  was. In 4 of 9 runs it edited nothing and answered that it found no pending work («No he
  encontrado ningún trabajo pendiente que continuar, así que no he cambiado nada»).
- **The document** gives the next session the request as written and what the last one did:
  every run cut early finished in session 2, with every exact text and the change met.
- **Repeating the request** gets most of the way, one session later and less surely (one run
  stayed at 8 of 24).
- **The flow** spends the cap on its roles: its first session meets less (38 %) and
  `gym-bookings` never recovered.
- **The layer** spends its first session reading and consulting its memory (5.6 files read
  before acting, against 2.1 for solo) and meets 20 %; its memory did not carry the task
  across sessions any better than the code did.

## Sonnet and Opus, cap 20

| model | arm | runs | S1 | S3 | Δ S3 vs solo | 95 % | cut sessions per run | $ / run |
|---|---|---|---|---|---|---|---|---|
| Sonnet | solo | 6 | 95 % | 95 % | | | 0.0 | 0.57 |
| Sonnet | repeat | 6 | 96 % | 96 % | +1 | [−2, +4] | 0.0 | 0.75 |
| Sonnet | doc | 6 | 97 % | 97 % | +2 | [0, +6] | 0.0 | 0.68 |
| Opus | solo | 3 | 99 % | 99 % | | | 1.3 | 1.99 |
| Opus | doc | 3 | 100 % | 100 % | +1 | [0, +4] | 1.0 | 2.06 |

These cases fit in one session for both: Sonnet writes whole files from a few shell commands
(8–14 actions, never cut), and Opus, when cut, had already met the requirements and was
checking its work. With nothing left to continue, no arm can help.

## After the plan: Sonnet, cap 5 (3 cases × 2 repeats)

To see whether the result holds for a stronger model that is cut, the same arms with a cap
that cuts Sonnet mid-work. Not pre-registered; read it as a check, not a decision.

| arm | S1 | S2 | S3 | Δ S3 vs solo | 95 % | cut early → 20/24 | $ / run |
|---|---|---|---|---|---|---|---|
| solo | 6 % | 6 % | 6 % | | | 0/6 | 0.26 |
| repeat | 17 % | 32 % | 47 % | +41 | [+33, +50] | 1/5 | 0.36 |
| doc | 5 % | 19 % | 51 % | +45 | [+35, +50] | 3/6 | 0.34 |

Same order as haiku: alone, a cut task does not move; the request or the document gets it
moving. At this cap `doc` and `repeat` are within 5 points.

## Decision (as fixed)

- **Build the task document.** `doc` beats `solo` by 38 points after session 3 with haiku
  (interval [+26, +53]), with no more regressions, at 1.08× the cost. `repeat` is 9 points
  behind (more than 5), and the document also wins on the change (100 % against 89 %) and on
  runs cut early (5/5 against 3/4).
- **Flows stay an advanced option.** Read to the letter, the rule is not met: `flow` is above
  `solo` (+13, interval [−6, +29], which takes in zero) and the layer. But it is below `doc` and
  `repeat` on requirements met, the change and cost per requirement, it costs the most per run,
  and it is the only arm with regressions. A task document gets more for less, so a flow is
  not what to build for long tasks.
- The layer, as a reference: below `solo`.

It matters only when a task does not fit in one session: Sonnet and Opus finished these cases
in their first. The document costs almost nothing when it is not needed (it is the request
plus a few lines per session), so it can be on by default for tasks that span sessions.

Limits: three cases; with haiku the arms are decided by a few runs cut early (4–8 per arm),
so the intervals are wide; the cap is a stand-in for a session that ends (a limit, a crash,
the user closing it); one model family; edits made from the shell are not counted as edits,
so «files read before the first edit» is missing for Sonnet.
