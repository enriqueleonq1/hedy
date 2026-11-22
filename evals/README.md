# Evals

Real tasks to measure whether each Hedy layer (project memory, code map) improves the
results. Run them with `cargo run -p hedy-evals -- --model haiku` (see `--help`); every
run is a real agent session and spends tokens of the chosen account.

Each case is a folder:

- `case.json`: `title`, `prompt` (what the user asks), `check` (shell command; exit 0
  passes), optional `notes` (memory for the `memory` and `full` layers), optional
  `teamNotes` (team-layer notes: the project joins a team made for the run, in a bare repo
  next to it), `measures` and
  `timeoutSecs`.
- `repo/`: the starting project. Each run copies it into a fresh git repo.
- `check/`: hidden checks, copied into the worktree only after the agent finishes.

Results are appended as JSON lines to `evals/results/` (not committed).

Flows (docs/decisions.md §45, §50): `--flows none,classic,file:<path>` runs each case as one
agent, as a template or as a flow written as text; `--roles <path>,...` saves custom roles
those flows use. Each run's `steps` records every role's tokens and every check's result.
`experiments/value-meter/` holds the flows, roles and analysis scripts of §50
(`analyze.py` for the value meter, `compare.py` for cost per solved task).
