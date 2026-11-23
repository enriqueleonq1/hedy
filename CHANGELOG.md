# Changelog

All notable changes to Hedy are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.7.1] - 2026-11-23

### Fixed

- The terminal shows colors again in the installed app: the prompt, the shell's syntax colors
  and its suggestions (fish's gray autosuggestion read as typed text before).

## [1.7.0] - 2026-11-22

### Added

- A flow task shows what it spent: the run's tokens, time and cache share beside its name, and
  each step's tokens and time under its steps.
- When a flow goes back as many times as allowed, its task says so in a card, with what was
  last asked (the failing tests or the review), and offers one more round in the composer. It
  used to read as an error.
- In a project's Flujos tab, a flow that over ten closed tasks spent at least twice what a task
  with one agent spends there, without being accepted more often, says so, with a way to the
  Essential template or to stop hearing about it.
- A review comment can be saved, reworded, as a rule of the project's memory.
- `hedy flow import <file>` saves a flow written as text as yours; it starts by hand.

### Changed

- A flow with a single role no longer gets the shared board, and the first role is no longer
  asked to compare with a commit: in Hedy's measurements this cut a light flow's tokens by about
  a quarter, with the same results.

## [1.6.0] - 2026-11-22

### Changed

- Fewer flow templates, each with a reason: **Esencial** (recommended: one programmer who
  writes the tests first, and the tests as the gate), **Con tu aprobación** (a plan and your
  OK before any code) and **Revisión acotada** (plus a reviewer that sends the work back only
  for a concrete error, once). Each says how light or heavy it is. Flows copied from the old
  templates keep working.
- Picking a model says how fast and how costly it is ($, $$, $$$), and a flow step suggests
  its account's models as buttons.

### Added

- The flow editor notes when a flow has more than one AI reviewer: in Hedy's measurements
  more than one cost more without a better result. It never blocks saving or running.
- Evals: `--flows file:<path>` runs a flow written as text, `--roles` adds custom roles, and
  each run records every step's tokens and every check's result. Ten new cases, four of them
  hard (several files, a hidden security risk, a migration, bugs found from symptoms).

## [1.5.0] - 2026-11-21

### Added

- The orchestrator: a team of AIs on one task, step by step. A project's new **Flujos** tab
  shows a flow on a canvas, with moving neon lines. Its steps can be roles
  (architect, programmer, tester, reviewer, security, or your own), your approval, checks
  (the tests pass, the changes follow your rules, the reviewers approve) and reviews at once.
  A failed check goes back to an earlier step, at most a few rounds. Hedy's templates (Pipeline
  clásico, Con bucle de corrección, Doble revisión) are a click away; duplicate one to change
  it. Problems are shown on their step as you edit, with undo and a list view.
- Roles share a board (the plan, the test output, the reviewers' verdict) and each gets what
  it needs: read-only roles get the task's changes in their prompt. Tests run isolated, with
  no network. A reviewer's verdict is a strict last line, never guessed from free text.
- A flow task, live: its steps over the timeline (the one at work alive), the round when it
  went back, each check's result, the board, and a "Tu aprobación" card (Enter approves, E
  says what to change).
- A flow can follow a task's end: switch it on in the flow and a project task that ends with
  changes goes on through it, in its same copy (it spends the roles' tokens).
- From the terminal: `hedy flow list|show|run|new|edit` (edit in `$EDITOR`, checked on save)
  and the role library, `hedy roles list|show|new|edit|remove`.
- With a team: admins share flows and roles (`hedy flow share`, `hedy roles share`, or
  "Compartir con …" in the app), encrypted like team notes and checked on every PC. A team
  flow's own test command runs on a PC only once its user allows it there.

### Security

- A model name that would read as one of a CLI's options (`--yolo`) is refused everywhere a
  task, conversation, flow or handoff starts.
- A team role can never take the place of one of Hedy's roles, and nothing shared with a team
  may hide characters.
- A flow's seat only offers accounts the project may use (protected projects keep theirs).

## [1.4.0] - 2026-11-20

### Changed

- `.env` works like in Claude Code: the AI reads it unless you say otherwise. Each project has
  "Archivos que la IA no lee" in its settings (or `hedy block`), empty at first, with
  "Agregar los de siempre (.env, claves, credenciales)". The list covers every way an agent
  reads (Claude's own deny rules and Hedy's hook, permission cards, the API agent, `@`
  mentions, the code views). Task copies get your ignored `.env` files, unless listed, so tests
  and dev servers work. Hedy still never puts a secret's contents in what it sends an AI on
  its own (commit messages, rule checks).
- Asking and researching never create a branch. With Codex, Gemini CLI, Kimi Code or
  Antigravity a question works in a copy without a branch that is deleted when each turn ends;
  the branches and copies older questions left behind are cleaned up when the app starts.
- The whole app was redesigned, simpler and calmer: Inicio as one inbox, a home for each
  project, one state pill per task, a card at the end of a turn, one Memoria, settings with an
  index and a sheet per project, Hedy's own neutral themes and the Geist type.

### Added

- Permission modes, as in Claude Code: Manual, Aceptar ediciones and Automático, in the
  composer's «Permisos» chip, per project in its settings, or with `hedy permissions`. In a
  task that changes code, Automático runs edits and commands without asking inside the task's
  copy; risky commands (`sudo`, `curl`, `git push`, a `publish`…), anything outside the copy,
  `.git`, the agents' own settings and the files the AI does not read keep their card.
- A question asked for a document or a page writes it in the task's own folder, outside the
  project, and the chat lists it with «Abrir» and «Guardar en…». Only documents, pages, images
  and data open from Hedy.

### Security

- A question's writes are kept in its folder by Claude's hook, whatever allow rules the
  account or the repository's Claude settings have.
- A project whose settings cannot be read falls back to the usual blocked files, never to none.

## [1.3.0] - 2026-11-20

### Security

- Agents never read a protected file (`.env`, keys) through a search over a whole folder, a
  search glob, a link or a shell pipeline, and every `hedy` command that starts an agent now
  runs it with that guard (before, only `hedy ask` did). One-shot requests (commit messages,
  summaries) run with no shell.
- A protected file (`.env`, keys) in a folder whose name has a space or accents was read under
  another name, and its contents could go in `hedy commit`'s request. Git's diffs no longer run
  decrypting textconv filters, whatever the user's diff settings.
- Commit message suggestions ("Sugerir con IA", `hedy commit`) no longer send removed lines (a
  secret being taken out) nor a protected file's contents in a task's changes, and file names
  are printed without their control characters.
- An AI can no longer propose a skill that runs commands as soon as Claude Code loads it
  (`!`…`` or a ```` ```! ```` block), and skills from your other PCs pass the same checks as
  local ones.
- "Permitir en este chat" no longer covers the agents' own settings (`.claude/`, `.mcp.json`,
  `.gemini/`…, where hooks and MCP servers start commands): each change there asks. No agent
  writes into `.git`, and a write that leaves the task folder through a link (the shared
  `node_modules`) is denied.
- Hedy's own git in a task's copy uses the copy's real git data, whatever its `.git` file says:
  a run (or a skill trial's check) could point it at a folder whose settings run a command.
- An AI cannot add or publish team notes, invite, change roles, remove people or skills, or
  leave a team through `hedy` from its session, and a trial's check counts as its session.

### Added

- Import and export skills as `SKILL.md` files, the format every CLI reads:
  `hedy skills import <path>` shows first what goes in, what does not and why (commands that
  run on load, secrets, names you already have, files Hedy does not keep), then asks;
  `hedy skills export --to <folder>` writes plain copies and never replaces a file.
- Whether a skill helps or gets in the way: on import and with `hedy skills check <name>`,
  Hedy measures it (size, a description that fits every task, overlap with your skills,
  dangerous commands, missing files) and an AI reads it against your notes, quoting what
  goes against them, what is vague and what really helps.
- A skills section in the app: every skill in one place (global and every project's), with a
  search and filters, and the chosen one in detail: what it costs in context, what Hedy
  measures, what the AI thinks against your notes, its last trial, and its steps. Import,
  export and try skills from there. «Usar también fuera de Hedy» writes a project skill
  where your usual Claude Code and other CLIs read it too (kept out of git); off by default.
  Checkboxes and radio buttons look the same everywhere.
- Try a skill for real: `hedy skills try <name>` runs up to 3 tasks twice each, with and
  without the skill, in throwaway copies of the project, and says from what the runs did
  (finished, the same check Hedy runs in each copy, the skill used, tokens) whether it is
  useful, doubtful or harmful. Before the check, the tests a run changed are put back, and on
  Linux the check runs isolated with bubblewrap (no network, writing only in its copy). A
  limit, a run you stop or a copy of the skill outside Hedy keeps no verdict. Nothing reaches the project; `hedy skills show` keeps the result until
  the skill changes. From a script (`--yes`, no terminal), each copy may change its own files
  but runs no command. A trial whose terminal was killed is cleaned up by the next Hedy (the
  app, `hedy tasks` or another trial).
- Personal sync: the same Hedy on every PC you use. Notes, proposals, skills, templates and
  themes travel through a private git repo of yours (GitHub, GitLab, Bitbucket, Gitea or your
  own server), encrypted and signed on each PC; the server never sees a note. Set it up in
  Ajustes → Sincronización or with `hedy sync setup <url>`; another PC joins with the 12 words.
  You choose, per project, whether its memory is yours, a group's or stays on one PC. Edits
  made apart are shown side by side to pick one, and a lost PC can be removed. See
  [docs/sync.md](docs/sync.md).
- Team memory: a team's shared decisions, conventions and rules that every agent in the team's
  projects follows, in a private git repo of the team's. It has three layers: the team's
  (always in the agent's context, quoted with its author), each project's, and your own.
  People join with a one-time invite code, members suggest changes and admins review them, and
  removing someone changes the key. Every PC checks who was allowed to write what. In
  Ajustes → Equipos or with `hedy team`. See [docs/team.md](docs/team.md).
- Ask your memory: a question answered only from what is written in your notes, your
  projects' and your teams', each claim citing its note (who wrote it, where it lives, when).
  When nothing written answers, it says so, and can look in the project's code and propose
  the note that would have answered it, for you to keep or discard. In the app, "Tu memoria"
  shows the answer beside a living graph of the memory, in the style of Obsidian's, that
  lights the notes it used. From the terminal, `hedy memory ask`.
- Check a change against your rules: the lines a change adds are compared with the written
  rules of the project and the team, and each one that goes against a rule gets a warning
  under it, citing the note. It only warns. In Cambios, "Revisar con tus reglas"; ask the AI to
  fix a warning, mark it as fine, or, when the rule no longer applies, change it (a team's is
  proposed to whoever may change it, or changed for the whole team if you may) or add an
  exception for the project. From the terminal,
  `hedy review` (uncommitted work, `--staged`, a commit or a range), also in a git hook.
- A context-saving package, on by default, measured on real tasks before turning it on: it cut
  cost by 30–41 %. Turn it off in Ajustes → Ahorro or with
  `hedy config set context_saving off`; `hedy doctor` says whether it is on.

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
- Approved skills stay inside Hedy: Claude Code gets them as a plugin for Hedy's sessions
  only (`/hedy:<name>`), the other CLIs in each task's own folder. Your project and your
  usual `claude` or `codex` outside Hedy no longer see them, and the copies an older Hedy
  left in your project are removed unless you edited or committed them.

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

[Unreleased]: https://github.com/enriqueleonq1/hedy/compare/v1.7.1...HEAD
[1.7.1]: https://github.com/enriqueleonq1/hedy/compare/v1.7.0...v1.7.1
[1.7.0]: https://github.com/enriqueleonq1/hedy/compare/v1.6.0...v1.7.0
[1.6.0]: https://github.com/enriqueleonq1/hedy/compare/v1.5.0...v1.6.0
[1.5.0]: https://github.com/enriqueleonq1/hedy/compare/v1.4.0...v1.5.0
[1.4.0]: https://github.com/enriqueleonq1/hedy/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/enriqueleonq1/hedy/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/enriqueleonq1/hedy/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/enriqueleonq1/hedy/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/enriqueleonq1/hedy/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/enriqueleonq1/hedy/releases/tag/v1.0.0
