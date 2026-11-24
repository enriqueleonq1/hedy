# Confirmation test: does the task document beat what Hedy does today?

Written before any run of this test (Nov 24, 2026). Two values are set by the pilot, by the
rule written below (the budget per session and the repeats); they go in their own commit
before the first main run. Anything else that has to change gets a commit with its reason
before going on.

**Question.** Test C (docs/decisions.md §57) found that a task document took haiku from 61 %
to 99 % of the requirements of a long task over three sessions. Its weak points: three cases
we wrote, a cut at 20 actions with a bare «Continúa.» (which is not what Hedy sends), mostly
haiku, and a third-party layer not set up as its docs say. This test asks again, with cases
we did not write, a cut by budget, Sonnet as the main model and the layer set up as its
documentation recommends:

- does the task document beat what Hedy does today when a task goes on in another session?
- do the third-party layer and the flow fall short of the document?

## Cases

From **Commit0** (github.com/commit-0/commit0 at `1123db3`, MIT; dataset
`wentingzhao/commit0_combined` at `944c325`): each task is a Python library whose functions
were emptied to their docstring and `pass` (and those without a docstring removed), graded by
the library's own test suite.

**Pool:** the dataset's 54 libraries whose stubs remove 1 000–3 500 lines (GitHub's line
count of the stubbing commit). The upper bound keeps the cost inside the budget. That is 17
libraries.

**Order:** `random.Random(20261124).shuffle(sorted(pool))`.

**Rules of eligibility**, checked in that order by `make_cases.py`; the first 8 eligible
libraries are the cases and the 9th is the pilot's:

1. A permissive license (MIT, BSD, Apache-2.0, ISC) at the repository's root, so the case can
   live in this public repository.
2. Its tests need no network, no external service and no real terminal.
3. Its dependencies and its tests' install with pip on this machine's Python (3.14), with
   pytest 8.3. Each case has its own environment, without the library itself.
4. The library is not left installed as some other package's dependency.
5. The reference (the commit before the stubs) passes at least 95 % of at least 50 tests
   here.

Result (`make_cases.py`, Nov 24, 2026):

| order | library | verdict |
|---|---|---|
| 1 | graphene | out (rule 3: its test dependencies pin a pytest that does not run on 3.14; with what is left, its tests do not import) |
| 2 | requests | out (rule 2: httpbin) |
| 3 | **sqlparse** | case: 461/463 reference tests pass, 55 requirements |
| 4 | loguru | out (rule 3, as graphene: its tests need `freezegun`, in no list that installs) |
| 5 | **voluptuous** | case: 148/148, 15 requirements |
| 6 | fabric | out (rule 5: 328/401) |
| 7 | bitstring | out (rule 3: its test requirements conflict with pytest 8.3) |
| 8 | minitorch | out (rule 1: no license file) |
| 9 | **marshmallow** | case: 1228/1229, 68 requirements |
| 10 | **more-itertools** | case: 662/663, 149 requirements |
| 11 | **cookiecutter** | case: 367/371, 64 requirements |
| 12 | pyjwt | out (rule 5: 147/262) |
| 13 | **python-rsa** | case: 86/87, 27 requirements |
| 14 | pexpect | out (rule 2: pty) |
| 15 | **tinydb** | case: 201/201, 17 requirements |
| 16 | **attrs** | case: 1321/1335, 85 requirements |
| 17 | imapclient | the pilot: 267/267, 46 requirements |

The grader was checked on each case: the start meets 0 requirements (cookiecutter 3, python-rsa
1, from what the stubs left), and the reference meets all of them (marshmallow 67 of 68: two of
its tests depend on the time of day).

Each case is in `evals/confirm/<library>/`:

- `repo/`: Commit0's start without its tests (and without the docs' PDF, whose source is in
  `docs/`).
- `check/`: the tests, the change's test and the grader (`confirm_check.py`).

The agent never sees the tests.

**Requirements** are the hidden tests in groups: one group per test class, and a file's loose
test functions in groups of ten (in file order). A requirement is met when every test of its
group passes. Only tests that pass with the reference count.

**The change.** Each case has one change, written here before the runs. It is asked with the
second session in every arm, and its own hidden test is `CHG`. It is an addition the rest of
the library does not need, so «respected» means it was done and kept, whatever the rest of
the work did. Each was checked against the reference with the change made by hand.

| case | change asked in session 2 |
|---|---|
| sqlparse | `sqlparse.utils.strip_line_comments(sql)`: the text without `--` line comments (the line break stays), except inside single-quoted strings |
| voluptuous | the validator `voluptuous.validators.Even(msg=None)`: an even int passes as is; anything else raises `Invalid` with `msg` or «expected an even number» |
| bitstring | `bitstring.utils.popcount_hex(s)`: the bits set in a hex string, with or without `0x` (`''` → 0); `ValueError` if not hex |
| marshmallow | the validator `marshmallow.validate.Even(error=None)`: returns an even int; else `ValidationError` with `error` or «Must be an even number.» |
| more-itertools | `more_itertools.pairwise_sums(iterable)`, exported: the sum of each pair of consecutive items |
| cookiecutter | `cookiecutter.utils.slugify_project_name(name)`: lowercase, each run of non-ASCII-alphanumerics → one `-`, no `-` at the ends |
| python-rsa | `rsa.common.is_power_of_two(n)`: `True` for positive powers of two, `False` otherwise, `TypeError` if not an int |
| tinydb | `tinydb.utils.chunked(iterable, size)`: lists of `size` items in order, the rest in the last; `ValueError` if `size` < 1 |
| attrs | `attr.converters.strip_whitespace(value)`: a `str` stripped; anything else as is |
| imapclient (pilot) | `imapclient.util.quote_mailbox(name)`: the name in double quotes, each backslash and `"` in it escaped with a backslash |

No real task from the user's projects was given, so there are no separate cases.

## Sessions

Each session is a new task: a new agent process with no resume and nothing shared but the
code. What a session leaves is committed before the next one.

**The cut.** A session is stopped once it has spent a budget of tokens at input price
(`--budget`). Output counts 5×, a cache read 0.1× and a cache write 1.25×, as Claude bills
them, so the budget is the same for every model. The runner weighs the session's logs every
3 s. This stands in for an account reaching its limit.

**Sessions per run.** The change comes with session 2. A run ends when a session from the
second on ends by itself (not cut), or after 6 sessions.

**The messages.** Session 1 is the request as written (`case.json`). From session 2 on, each
arm sends its own way to go on, followed in session 2 by the user's change: «Además, un
cambio al pedido: <change>».

## Arms

1. **hedy-hoy** (`today`): what Hedy does today when a task goes on in another session.
   - That is the handoff (`Hedy::handoff_task`, crates/hedy-core/src/application/tasks.rs:1692,
     prompt in crates/hedy-core/src/domain/prompts.rs:311): «## Pedido original» with the
     request, then «## Contexto»: «Claude Sonnet estaba trabajando en esta tarea y se detuvo.
     Archivos cambiados hasta ahora: … Revisa el estado actual de la carpeta y continúa donde
     quedó.»
   - Messages the user sent after the request do not go with it.
   - («Esperar» at a limit resumes the same session (crates/hedy-core/src/application/waits.rs:192),
     which is not a new session and is not tested here.)
2. **pedido-repetido** (`request`): the request as written, then «Continúa.».
3. **documento** (`doc`): §57's task document, after «Continúa.»: the request as written, the
   change once asked, and each session's progress (files changed, whether the project imports,
   the agent's last message).
4. **capa-de-terceros** (`layer`): hedy-hoy's messages, with the third-party layer of §50.6
   installed in the test account as its documentation recommends:
   - version 5.0.0 (tag commit `4716b3c`), the latest on Nov 24, 2026;
   - `install --agent claude-code --preset full-gentleman --persona gentleman`;
   - telemetry off;
   - its memory server and MCP servers registered;
   - its review hook left on, as shipped.

   Its docs prescribe no resume command: the agent tracks a task it judges large in a task
   file and its memory, and recalls them when the user refers to past work, which hedy-hoy's
   message does. Same cut, no cap of its own.
5. **flujo** (`flow`): hedy-hoy's messages, each session run as Hedy's `essential` flow, under
   the same budget. It runs only if the budget is left after the rest.

## Models, repeats and order

- **Sonnet** (main): every case × arms 1–4, 3 repeats, unless the pilot's projection says
  fewer.
- **haiku** (support): the same, 2 repeats.
- **Opus** (signal only): 1 run on 2 cases (the first two of the order: sqlparse, voluptuous),
  arms documento and capa-de-terceros.
- **Order:**
  1. Sonnet;
  2. haiku;
  3. Opus;
  4. flow, with Sonnet, 1 repeat, if money is left.

  Each stage starts only if the money it needs is left.

## Pilot

imapclient × hedy-hoy, documento and capa-de-terceros × 1 Sonnet run, with a budget of
300 000 tokens per session. It validates the harness, calibrates the cut and projects the cost. Its
results are left out of the analysis.

- **The cut.** If Sonnet's hedy-hoy or documento run finishes in session 1, the budget is
  halved. If it is still cut in session 6, the budget is doubled. Either way the pilot runs
  again, at most twice. Otherwise the budget stays. (imapclient is a middle-sized case: the
  largest ones may need up to the 6 sessions.)
- **The repeats.** The cost of a Sonnet run is the pilot's mean. Repeats for Sonnet are the
  largest r ≤ 3 such that the following fits in $60:
  - the pilot;
  - Sonnet: 8 × 4 × r runs;
  - haiku: 8 × 4 × 2 runs, at haiku's price per token;
  - Opus: 4 runs, at Opus's price.

  If r = 1 does not fit, Opus goes first, then haiku's second repeat. If it still does not
  fit, Sonnet runs case by case in the order above until the money ends.
- **The stop.** The spend is counted after every run, with the CLI's estimates; a session cut
  by the budget is priced at its model's median rate. At $60 everything stops and what is
  there is reported.
- **Usage limits.** A run a usage limit cut is voided and run again. Voided runs are counted
  and reported, never used.

## Measures (per run)

- **Requirements met after its last session** (main), as a share of the case's groups.
- **Task finished:** every requirement and the change met.
- **Change respected:** `CHG` met after the last session.
- **Regressions:** requirements met after one session and not after the next, summed.
- Cost in USD, tokens and sessions used.
- The final diff of each run (`--diffs`, `…-final.patch`), and each session's.

A case is left out of the analysis if Sonnet's first session ended without being cut in most
of its runs: Sonnet did not need more than one session there.

## Analysis

`analyze.py`:

- For each arm against hedy-hoy and against documento, the mean over cases of the paired
  difference (each case's mean over its runs). Its 95 % interval comes from a bootstrap over
  cases (10 000 resamples, seed 7).
- Everything by case as well as in total, to see if one case drags the result.
- The same for haiku. Opus is reported by run.

## Rules of decision (as given, fixed now)

- Documento confirmado si, con Sonnet, gana a hedy-hoy por ≥ +10 puntos con IC inferior > 0,
  coste ≤ 1,2× y no más regresiones que hedy-hoy.
- Si documento ≈ pedido-repetido (diferencia dentro del IC), se construye primero la versión
  mínima: el pedido tal cual en cada sesión nueva; el avance queda como mejora.
- Capa de terceros o flujo: si alguno gana al documento por ≥ +10 con IC inferior > 0, NO se
  borran los flujos y se estudia qué pieza aporta. Si quedan dentro del ruido del documento y
  cuestan más, o por debajo, se confirma quitar los flujos.
- Si hedy-hoy ya está a ≤ 5 puntos del documento, el documento no se construye como
  prioridad.

How each rule is read:

- **«More regressions»:** the paired mean of regressions per run.
- **«documento ≈ pedido-repetido»:** the 95 % interval of their difference includes 0.
- **The flow:** if the flow does not run (money), the rule about flows is decided by the layer
  alone and that is said.

## Accounts and data

- **Accounts:** runs spend only the account at `~/.claude-personal`. The test accounts in a
  temporary Hedy data folder link to it; the layer's account links to its credentials file.
- **Data:** Hedy's real data is never touched (`HEDY_DATA_DIR` temporary).
- **Environment:** each case's Python environment is outside the repository and is put first
  on the agent's `PATH`, so the agent's `python3` has pytest and the dependencies.

## Amendment 1 (Nov 24, 2026, after the Sonnet pilot, before any main run)

**Why.** The Sonnet pilot (imapclient, 300 000 tokens per session) ran as follows:

- hedy-hoy was cut in all six sessions and met 3 of 46 requirements;
- documento met 8 of 46 by its fourth session and then ended by itself.

Sonnet spends most of a session reading these libraries before it writes. The rule above
says to double the budget. At Sonnet's price, a run would then cost about $7, and the main
runs about $220 even with one repeat, against $60. The user chose to keep the $60 and make
haiku the main model.

**What changes:**

- **Main model:** haiku, every case × arms 1–4 × 3 repeats.
- **The decision rules apply to haiku** instead of Sonnet.
- **The cut is calibrated by a haiku pilot:** imapclient × hedy-hoy, documento and
  capa-de-terceros × 1 run, starting at 600 000 tokens per session.
  - If hedy-hoy or documento ends by itself in session 1, the budget is halved and the pilot
    runs again (at most twice).
  - Otherwise the budget stays.
  - Its value goes in a commit before the main runs.
  - These libraries are larger than what haiku finishes in six sessions, so «2–4 sessions» is
    not the target any more. The test measures how far each way of going on gets in at most
    six sessions under a budget.
- **Order:**
  1. haiku;
  2. the flow with haiku, 1 repeat (8 runs);
  3. Sonnet as a signal only, if at least $8 is left: hedy-hoy and documento on voluptuous (the
     smallest case), 1 run each, 300 000 tokens per session.
- **Opus is dropped.**
- The Sonnet pilot's runs are reported as a signal and stay out of the analysis.

## Amendment 2 (Nov 24, 2026, 09:30, before any main run)

The account at `~/.claude-personal` reached its five-hour limit during the haiku pilot. Its
three runs were voided. At the user's word, runs from now on spend `~/.claude`, the account of
the session that drives them. The model and everything else stay as they are.

## Amendment 3 (Nov 24, 2026, 11:40, during the haiku pilot, before any main run)

A haiku run takes about 30 minutes (six sessions of about five), so 3 repeats were about
eight hours of runs. To make it shorter, the user chose **2 repeats** for haiku's main runs
(8 cases × arms 1–4 × 2 = 64 runs), with more runs at once. The intervals will be wider.
Nothing else changes.
