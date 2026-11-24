# Evals

Real tasks to measure whether each Hedy layer (project memory, code map) improves the
results. Run them with `cargo run -p hedy-evals -- --model haiku` (see `--help`); every
run is a real agent session and spends tokens of the chosen account.

Each case is a folder:

- `case.json`: `title`, `prompt` (what the user asks), `check` (shell command; exit 0
  passes), optional `tests` (the project's own tests, as the agent can run them), optional `notes` (memory for the `memory` and `full` layers), optional
  `teamNotes` (team-layer notes: the project joins a team made for the run, in a bare repo
  next to it), `measures` and
  `timeoutSecs`.
- `repo/`: the starting project. Each run copies it into a fresh git repo.
- `check/`: hidden checks, copied into the worktree only after the agent finishes.
- `reference/` (optional): a solution that passes, as the files that change on top of `repo/`.
  Never shown to the agent.

Results are appended as JSON lines to `evals/results/` (not committed). With `--diffs`, each
run's final patch goes to `evals/results/diffs/<results file>/<run>.patch`, with the hidden
check's verdict beside it in a `.json` of the same name.

Flows (docs/decisions.md §45, §50): `--flows none,classic,file:<path>` runs each case as one
agent, as a template or as a flow written as text; `--roles <path>,...` saves custom roles
those flows use. Each run's `steps` records every role's tokens and every check's result.
`experiments/value-meter/` holds the flows, roles and analysis scripts of §50
(`analyze.py` for the value meter, `compare.py` for cost per solved task).

Rescue (`--rescue`, `experiments/rescue/`): after the agent, the project's `tests` run and, when
they fail, the run goes on in three arms (the same session told the failure, a new session told
it, and a second attempt without it); four results per run, sharing the first attempt.
`experiments/rescue/stability.py` checks a case's tests and hidden check on its start and its
reference before using it.

Long tasks (`--long <arms>`, cases in `long/`, `experiments/long-tasks/`): each run is three new
sessions (`solo`, `repeat`, `doc`, `flow`), each cut after `--cap N` actions. With
`--sessions N` it is the confirmation test (cases in `confirm/`, `experiments/confirmation/`):
arms `today` (Hedy's handoff), `request`, `doc` and `flow`; the change is asked with the second
session and the run goes on until a session ends by itself, at most N. `--budget T` cuts a
session once it has spent T tokens at input price (output 5×, cache read 0.1×, cache write
1.25×), weighed from its logs. `experiments/confirmation/make_cases.py` builds the `confirm/`
cases from Commit0 by the rule in its `PLAN.md`.

Checkpoints (`--checkpoints solo,informed,plan,approval`, `experiments/checkpoints/`): a case's
`checkpoint` in `case.json` holds the request as asked there and what the user adds at the plan.
`plan` (plan first, the same session implements) and `approval` (the «Con tu aprobación» flow)
answer their first plan or approval with it and approve the next; `informed` asks it up front.
