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
