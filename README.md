# Hedy

Work with several coding AIs (Claude Code, Codex, Gemini, Kimi) and several accounts per
provider from one place. Named after Hedy Lamarr, inventor of frequency hopping.

Hedy never touches session tokens: each subscription account lives in the official CLI's
own config folder, and logging in always uses the CLI's official flow.

## Install (Linux)

```fish
cargo install --path crates/hedy-cli
hedy completions fish > ~/.config/fish/completions/hedy.fish
```

## Quick start

```fish
# The account already logged in at ~/.claude (adopted as is)
hedy account add "Claude trabajo" --claude --default-folder
# A second account in its own folder (opens the official login)
hedy account add "Claude personal" --claude
hedy account default claude-personal

cd ~/code/tienda; hedy use claude-trabajo    # add --protect to reserve it for this project
hedy run                                      # opens `claude` with the right account
hedy status                                   # accounts, logins and current project
```

Spanish aliases: `hedy estado`, `hedy usar`, `hedy cuenta agregar`.

## Desktop app

```fish
cd app; npm install; npm run tauri dev
```

See `docs/decisions.md` for what was verified about each CLI.
