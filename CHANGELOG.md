# Changelog

All notable changes to Hedy are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.2.0] - 2026-11-16

### Added

- Your account's own slash commands in Hedy: custom commands, skills and plugin commands
  (for example `/design`). Type `/` in the chat to see them next to your templates, with
  what each one expects. They run as they would in the CLI. Hedy learns them from what the
  CLI reports (Claude Code and the ACP agents), never by reading the account's folder.
- `hedy commands` lists an account's slash commands, and `hedy ask "/design …"` runs one.
- The terminal panel resizes like VS Code's: drag its top edge, double-click it or use
  "Maximizar" to maximize and restore. Hedy remembers its height.

## [1.1.0] - 2026-11-16

More of Hedy from the terminal.

### Added

- `hedy ask "…"` asks a quick question from the terminal and streams the answer. It is
  stored as an ordinary conversation, so it also shows up in Hedy → Conversations, where
  you can keep chatting or carry it into a project. `-c` follows up, `-m` picks the model,
  `-r` researches, `--json` is for scripts, and it reads from a pipe
  (`git diff | hedy ask "anything odd here?"`).
- `hedy usage` shows how much of its plan each account has used, like `/usage` in each CLI,
  with when each window renews. `hedy usage --short` prints one line for a shell prompt or
  a status bar (`CP 83% · CT 34%`).
- `hedy run` warns before opening a CLI whose account is at 90 % or more of a window, and
  asks before going on. It never switches accounts by itself.
- `hedy commit` suggests a commit message for what you staged, using one of your accounts,
  and commits with your own git (hooks and signing included). `--print` only prints it.

### Changed

- The app notices conversations started from the terminal without a restart.

## [1.0.1] - 2026-11-16

### Fixed

- Claude's Fable model is offered in the model menus, next to Opus, Sonnet and Haiku.

### Changed

- The install section of the README points Debian, Ubuntu and Linux Mint users to the
  `.deb` and the AppImage from the releases page.

## [1.0.0] - 2026-11-16

The first public release. Linux only for now.

### Accounts and providers

- Claude Code, Codex, Gemini CLI, Kimi Code and Antigravity, plus Kimi and Gemini by API key.
- As many accounts per provider as you have, each in the CLI's own config folder, logged in
  through the CLI's official flow. Hedy never reads or stores session tokens.
- Each project knows which account it uses; protected projects only use theirs (opt-in).
- Plan usage for every account, refreshed every three minutes, with its age on hover and a
  manual refresh. Hitting a limit never switches you to another subscription on its own.
- API keys live in the system keyring.

### Tasks

- Tasks run in parallel, each in its own git worktree, and nothing touches your folder until
  you accept it. Accepting can be previewed and undone.
- Plan, implement, test and review phases, each with its own account and model if you want.
- A queue for follow-ups, waits that resume on the same account after a limit, and desktop
  notifications.
- Images in messages (PNG, JPEG, GIF, WebP).
- Quick questions outside any project, which you can carry into a project with their context.

### Review and code

- Review a task like a pull request: diff with context, comments on any line, and one click
  to send them all back to the AI.
- File explorer, `Ctrl+P` to open files, `F1` for commands, text search, history, blame and
  "before the AI" to see a file as it was when the task started.
- Small edits in a CodeMirror editor; anything bigger opens in VS Code at the same line.
- Ship: commit with a suggested message, push and open the pull request (`gh` optional).

### Memory and skills

- Project memory served to agents over MCP. Agents only propose notes; you approve them.
- Learned skills and prompt templates, from the app or the `hedy` command.

### Look and feel

- A theme gallery: Hedy's own dark and light themes plus One Dark Pro, Dracula, Catppuccin,
  Nord, GitHub and Solarized. Click one and the whole app previews it before you apply it.
- Import a VS Code theme file, or describe the look you want and let one of your accounts
  draft it. Every theme is checked for readable contrast and fixed when it falls short.
- Spanish and English interface, focus mode, command palette and keyboard shortcuts.

### Command line

- `hedy account`, `hedy use`, `hedy run`, `hedy status`, `hedy tasks`, `hedy doctor`,
  `hedy skills` and `hedy template`.

[Unreleased]: https://github.com/enriqueleonq1/hedy/compare/v1.2.0...HEAD
[1.2.0]: https://github.com/enriqueleonq1/hedy/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/enriqueleonq1/hedy/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/enriqueleonq1/hedy/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/enriqueleonq1/hedy/releases/tag/v1.0.0
