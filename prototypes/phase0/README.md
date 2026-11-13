# Phase 0 prototypes

They launch the official CLIs with an account folder and translate what they emit into
Hedy's common event language. Not production code.

```fish
cargo build -p hedy-phase0
set P (pwd)/target/debug/hedy-phase0
set -x HEDY_SAMPLES /tmp/hedy-samples   # optional: stores every raw message in <cli>.jsonl
mkdir -p $HEDY_SAMPLES
```

## Without login (already tested)

```fish
$P claude-status --config-dir /tmp/empty-account          # logged_in=false, exit 1
mkdir -p /tmp/codex-empty; $P codex --home /tmp/codex-empty --account-only
$P gemini --home /tmp/gemini-empty                         # initialize + auth methods
$P pty --config-dir /tmp/empty-account --seconds 8 claude  # welcome screen
```

## With a real account (end-to-end test)

1. Official login into a new folder (opens the browser):

   ```fish
   set C ~/.local/share/hedy/accounts/claude-personal
   mkdir -p $C; env CLAUDE_CONFIG_DIR=$C claude auth login
   ```

2. A request that needs permission (asks in the terminal: `a`, `s` or `d`):

   ```fish
   mkdir -p /tmp/hedy-test; cd /tmp/hedy-test; git init -q
   $P claude --config-dir $C --cwd . "create hola.txt with a greeting, then list the files"
   ```

Codex and Gemini are not installed globally on this machine: pass `--binary <path>`.
Codex (`--home`, after `codex login`) and Gemini (`--home`, after `gemini` → Log in with
Google) work the same way. The Codex folder must exist before logging in.
