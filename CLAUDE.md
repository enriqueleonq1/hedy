# Hedy

Desktop app (Tauri v2 + React 19) and `hedy` command to work with several coding AIs
(Claude Code, Codex, Gemini CLI, Kimi Code, Antigravity, and Kimi/Gemini by API key) and
several accounts per provider. Gemini CLI, Kimi Code and Antigravity are driven over ACP.
Verified facts live in `docs/decisions.md`; read it before touching adapters.

## Commands

- Build: `cargo build`
- Tests: `cargo test`
- Lint (no warnings): `cargo clippy --all-targets -- -D warnings`
- Format: `cargo fmt`
- Install the CLI: `cargo install --path crates/hedy-cli` (binary `hedy`)
- Isolated manual testing: `HEDY_DATA_DIR=/tmp/hedy-test hedy status`
- Learned skills from the terminal: `hedy skills` (`list`, `show <name>`, `approve <id>`,
  `reject <id>`, `remove <name>`, `import <path> [--global] [--replace] [--yes]`,
  `export [names] --to <folder>`, `check <name>`: helps or gets in the way, `try <name>
  [--task …] [--check …]`: runs real tasks with and without it, spends tokens; `--yes`
  needs both and, with no terminal, allows only file changes inside each copy); in the UI,
  `#m=<path>&skills` opens a project's skills in Memoria
- Prompt templates from the CLI: `hedy template list|add <name> [--project] --text …|remove <name>`
- Ask the memory (docs/decisions.md §36): `hedy memory ask "<question>"` (`--team`, `--all`,
  `--code` to look in the project's files and propose a note, `--json`); answers only from the
  notes and cites each. In the app, `#ask` (`#ask=<path>` for a project). Real eval (spends
  tokens; use claude-personal): `scripts/ask-eval.py <claude-account-folder> [model]`
- Check a change against the written rules (docs/decisions.md §37): `hedy review` (uncommitted
  work; `--staged`, a commit or `main..HEAD`, `--task <id>`, `--json`); only warns, citing each
  note. In a pre-commit hook: `hedy review --staged || true`. In the app, Cambios → "Revisar con
  tus reglas". Real eval (spends tokens; use claude-personal):
  `scripts/rules-eval.py <claude-account-folder> [model] [runs]`
- Health check (CLIs and their versions, accounts, Hedy, environment): `hedy doctor`
- Running and recent tasks across projects: `hedy tasks`
- Personal sync (docs/sync.md): `hedy sync` (`setup <url>`, `join <url>`, `now`, `conflicts`,
  `resolve`, `owner`, `project`, `remove <pc>`, `leave`). End to end against an empty repo:
  `scripts/sync-e2e.sh <url>` (real binary) and
  `HEDY_E2E_URL=<url> cargo test -p hedy-app -- --ignored sync_ipc` (the app's commands)
- Team memory (docs/decisions.md §35): `hedy team` (`create <url> --name`, `invite [--admin|--mine]`,
  `join <url>` (code asked), `now`, `link`, `notes`, `add [--everyone]`, `publish <id>`, `review`,
  `role`, `remove`, `rename`, `leave [--here]`; `--team` picks one). End to end against an empty
  repo: `scripts/team-e2e.sh <url>` (real binary and keyring; three people)
- App (from `app/`): `npm install`, then `npm run tauri dev`; UI only with sample data:
  `npm run dev` (`?theme=light|dark`, `?lang=es|en`, `?onboarding=1`, `?data=empty|noprojects|fail`, `#accounts`, `#settings`,
  `#memory` (Memoria; `#m=<path>[&skills|pending|map]` for a project), `#ps=<path>` (a project's settings), `#pf=<path>[&<key>]` (its flows),
  `#settings/<section>`, `#sync`, `#teams`; Inicio is `#`)
- App checks (from `app/`): `npm run typecheck`, `npm run lint`, `npm test`
- Icons: edit `app/src-tauri/icons/logo.svg`, render it to `logo.png` (1024 px), then
  `npx tauri icon src-tauri/icons/logo.png`. Dev window icon on Wayland:
  `app/scripts/install-dev-icon.sh`
- Real end-to-end check (spends a few tokens; `HEDY_SMOKE_ACCOUNT=<folder>` picks the account,
  else `~/.claude`): `cargo run -p hedy-core --example smoke_task -- haiku` (`env` / `bash`: a
  listed `.env` stays unread; `free`: an empty list reads it; `doc`: a question's document lands
  outside the project; `auto`: a task in automatic mode shows no card)
- Files the AI does not read in a project (empty by default, as in Claude Code):
  `hedy block [patterns] [--remove|--usual|--clear]`
- What a project's new tasks do without asking (as Claude Code's modes):
  `hedy permissions [manual|edits|auto]`
- Orchestrator flows (docs/decisions.md §42–§44): `hedy flow list`, `hedy flow show <name|key>`,
  `hedy flow run <name|key> "<request>" [--model M]` (asks approvals and permissions here)
- Evals (real tasks, spend tokens): `cargo run -p hedy-evals -- --model haiku` (`--list`,
  `--case`, `--layers plain,memory,code-map,full`, `--repeat`); results in `evals/results/`
- Context-saving switches (`crates/hedy-core/src/experiments.rs`, docs/decisions.md §33): on by
  default; off with `hedy config set context_saving off` (or Ajustes → Ahorro);
  `HEDY_EXPERIMENTS=none` overrides the setting; compare them with
  `hedy-evals --variants base,none,base+read-guard` (judged by cost, use `claude-personal`)
- Install as a desktop app (Arch/CachyOS): `cd packaging/arch && makepkg -f`, then
  `sudo pacman -U hedy-*.pkg.tar.zst`
- Phase 0 prototypes: `cargo run -p hedy-phase0 -- --help`

## Layout

- `crates/hedy-core`: domain + application (ports) + adapters
- `crates/hedy-cli`: `hedy` binary (driving adapter)
- `crates/hedy-memory`: project-memory MCP server (rmcp, stdio; also the learned-skill
  tools `skill_list` / `skill_get` / `skill_propose`); the app binary serves it
  as `memory-mcp`, like Claude's `PreToolUse` hook (`app/src-tauri/src/main.rs`)
- `crates/hedy-evals`: eval runner; cases (`case.json`, `repo/`, hidden `check/`) in `evals/`
- `app/src-tauri`: Tauri commands (driving adapter; DTOs live here, not in the domain)
- `app/src`: React UI. Talks to the backend only through `api/HedyApi.ts`
  (`tauriApi` in the app, `MockApi` in tests and the browser)
- `identidad visual/`: official visual reference (screens, mascot, palette). Match it.
- `prototypes/phase0`: throwaway validation prototypes

## Architecture (hexagonal)

- `domain/`: pure types and rules. No I/O, no async runtime; only `thiserror` and
  `serde` (some domain types, such as `Task` and `AgentEvent`, are sent to the UI as is).
- `application/`: use cases, depending only on traits declared in `application/ports`.
- `adapters/`: implementations of the ports (filesystem, CLI processes, keyring, git).
- Driving adapters (CLI, Tauri) call use cases; they never call adapters directly.
- Unit tests use in-memory fakes of the ports (`application/testing.rs`); adapters get
  their own tests; `crates/hedy-cli/tests` runs the binary against a fake `claude`.

## Non-negotiable rules (enforced in code, not only in text)

1. Hedy never reads, copies or stores session tokens. Each subscription account lives in
   its official folder (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GEMINI_CLI_HOME`,
   `KIMI_CODE_HOME`); login is always the CLI's official flow. Hedy only picks the folder
   at launch. A CLI without a folder variable (Antigravity) gets one account per machine.
2. Official binaries, unmodified.
3. Never switch automatically from one subscription account to another on hitting a
   limit: warn and offer to wait, move to another provider, or use a paid API key.
4. Protected projects are opt-in (off by default): a protected project only uses its
   assigned account, and that account is not offered elsewhere. No account is blocked
   by default.
5. API keys in the OS keyring (`keyring` crate), never in files.
6. New memory notes are approved by the user: agents only read approved notes and
   only propose (`NoteStatus::Proposed`).
7. Local files Hedy writes in a repo go to `.git/info/exclude`.
8. Everything local: no server, no telemetry.

When launching a CLI, strip variables that change the account or billing
(`ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`, `OPENAI_API_KEY`, `CODEX_API_KEY`,
`GEMINI_API_KEY`, ...). See `docs/decisions.md`.

## Conventions

- Code, identifiers, comments, docs and commits in English. User-facing text in Spanish
  and English: `tr!("es", "en")` in Rust, `t("area.key")` in React with both tables in
  `app/src/i18n/messages/<area>.ts`. Never hard-code a visible string in one language.
- Colors only from `app/src/theme/tokens.ts` (contrast-tested); account colors are never
  used as text. Animations: transform/opacity/fill only, off with reduced motion.
- Small commits, Conventional Commits in one line: `type(scope): message`, e.g.
  `feat(cli): add hedy doctor`, `fix(app): keep the queue after a limit`. Types: `feat`,
  `fix`, `refactor`, `test`, `docs`, `style`, `chore`. Scope is the area (`core`, `cli`,
  `app`, `memory`, `skills`, `acp`, `evals`, `packaging`…), omitted when the change spans
  everything. No body and no co-author trailers.
- If official docs contradict the plan, say so and propose an alternative.
