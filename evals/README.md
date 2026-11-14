# Evals

Real tasks to measure whether each Hedy layer (project memory, code map) improves the
results. Run them with `cargo run -p hedy-evals -- --model haiku` (see `--help`); every
run is a real agent session and spends tokens of the chosen account.

Each case is a folder:

- `case.json`: `title`, `prompt` (what the user asks), `check` (shell command; exit 0
  passes), optional `notes` (memory for the `memory` and `full` layers), `measures` and
  `timeoutSecs`.
- `repo/`: the starting project. Each run copies it into a fresh git repo.
- `check/`: hidden checks, copied into the worktree only after the agent finishes.

Results are appended as JSON lines to `evals/results/` (not committed).
