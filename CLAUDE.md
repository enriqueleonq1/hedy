# Hedy

Desktop app (Tauri v2 + Svelte 5) and `hedy` command to work with several coding AIs
(Claude Code, Codex, Gemini, Kimi) and several accounts per provider.
Verified facts live in `docs/decisions.md`; read it before touching adapters.

## Commands

- Build: `cargo build`
- Tests: `cargo test`
- Lint (no warnings): `cargo clippy --all-targets -- -D warnings`
- Format: `cargo fmt`
- Install the CLI: `cargo install --path crates/hedy-cli` (binary `hedy`)
- Isolated manual testing: `HEDY_DATA_DIR=/tmp/hedy-test hedy status`
- Phase 0 prototypes: `cargo run -p hedy-phase0 -- --help`

## Layout

- `crates/hedy-core`: domain + application (ports) + adapters
- `crates/hedy-cli`: `hedy` binary (driving adapter)
- `crates/hedy-memory`: memory MCP server (phase 5)
- `app/`: Tauri + Svelte (phase 2+)
- `prototypes/phase0`: throwaway validation prototypes

## Architecture (hexagonal)

- `domain/`: pure types and rules. No I/O, no async runtime; only `thiserror` and
  `serde_json` (parsing).
- `application/`: use cases, depending only on traits declared in `application/ports`.
- `adapters/`: implementations of the ports (filesystem, CLI processes, keyring, git).
- Driving adapters (CLI, Tauri) call use cases; they never call adapters directly.
- Unit tests use in-memory fakes of the ports (`application/testing.rs`); adapters get
  their own tests; `crates/hedy-cli/tests` runs the binary against a fake `claude`.

## Non-negotiable rules (enforced in code, not only in text)

1. Hedy never reads, copies or stores session tokens. Each subscription account lives in
   its official folder (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GEMINI_CLI_HOME`); login is
   always the CLI's official flow. Hedy only picks the folder at launch.
2. Official binaries, unmodified.
3. Never switch automatically from one subscription account to another on hitting a
   limit: warn and offer to wait, move to another provider, or use a paid API key.
4. Protected projects are opt-in (off by default): a protected project only uses its
   assigned account, and that account is not offered elsewhere. No account is blocked
   by default.
5. API keys in the OS keyring (`keyring` crate), never in files.
6. New memory notes are approved by the user.
7. Local files Hedy writes in a repo go to `.git/info/exclude`.
8. Everything local: no server, no telemetry.

When launching a CLI, strip variables that change the account or billing
(`ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`, `OPENAI_API_KEY`, `CODEX_API_KEY`,
`GEMINI_API_KEY`, ...). See `docs/decisions.md`.

## Conventions

- Code, identifiers, comments, docs and commits in English. User-facing text in Spanish.
- Small, descriptive commits.
- If official docs contradict the plan, say so and propose an alternative.
