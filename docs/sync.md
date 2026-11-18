# Personal sync

The same Hedy on every PC of yours: approved notes and proposals, skills, prompt templates and
themes, through a private git repo of yours. Everything you keep is encrypted and signed on
your PC before it leaves it; only the repo's README and its key files (who the PCs are) are
plain. The design, the threat model and every decision behind it are in
[decisions.md §34](decisions.md).

## What syncs and what stays

| Syncs | Never syncs |
|---|---|
| Notes (approved and proposed) of your personal projects and the global ones | CLI logins, API keys, account folders |
| Skills, templates, themes | Projects whose memory is shared (a company's, a client's) or "Solo en esta PC" |
| Deletions | Usage counts and task history |

A project's memory is **personal** when every remote of its repo belongs to an owner you
answered "Solo mía" for (`github.com/your-user`). A project shared with a group, or one whose
owner you have not answered for yet, stays on each PC. The app asks once per owner. You can
also choose per project, in its memory screen or with `hedy sync project`.

## Set it up

1. Create an **empty private repo** (no README) on any git server: GitHub, GitLab, Bitbucket,
   Gitea or your own over SSH. Hedy refuses a repo that already has files. If your server has
   branch protection, turn it on so nobody can force-push.
2. Make sure your git can reach it **without a password prompt**: an SSH key in your agent,
   or a credential helper. Never put a token in the address: Hedy refuses such an address and
   keeps no password or token. Prefer **one credential per PC** (its own SSH key), so one PC's
   access can be revoked alone. Note that `credential.helper store` keeps a token in a plain
   file.
3. In the app, open Ajustes → Sincronización → **Configurar**, or run
   `hedy sync setup <address>`.
4. Keep the **12 words** (the recovery key) in your password manager. They are shown once,
   and nobody can recover them, not even Hedy.

On another PC: Ajustes → Sincronización → **Ya la uso en otra PC**, or
`hedy sync join <address>`, with the same address and the 12 words.

While the app is open it syncs on its own every 5 minutes. **Sincronizar ahora** (or
`hedy sync now`) syncs at once. The status bar shows the state: Sincronizado,
N para revisar, Sin sincronizar.

## Conflicts

When two PCs change the same item apart, the newer change is used everywhere. For notes and
skills, the other version waits under **Para revisar** (or `hedy sync conflicts`) for you to
choose. Templates and themes keep the newer one.

## Remove a PC, or leave

- **Quitar** next to a PC (or `hedy sync remove <name|id>`) makes a new key without it. From
  then on it cannot read what you keep, and its changes are no longer taken.
- **Also revoke its git access** (its SSH key or token on your git server). With push access
  a removed PC could still offer old keys to a PC that has not synced since, or fill the repo
  with junk.
- A removed PC keeps every key up to its removal: with any copy of the repo it can read
  everything written until then, even items it never synced.
- **Dejar de sincronizar en esta PC** (`hedy sync leave`) stops syncing here; your notes stay.
  Remove the PC from another one afterwards, so the others stop listing it.
- To come back, join again with the 12 words. It comes back as a new PC.

## Security in short

- **Encryption:** each item is encrypted with XChaCha20-Poly1305 under a key of the current
  epoch and signed by the PC that wrote it (Ed25519). The key is wrapped for each PC and for
  the recovery key.
- **Who can add or remove a PC:** only the 12 words add one; a PC can only remove PCs. Every
  key epoch must be signed by a member of the one before and keep the recovery key. A PC
  pins the epochs it has checked.
- **What the git server sees:** the PCs' random ids, how many there are, when each joined or
  was removed (the key files are plain), how many batches each wrote and when, file sizes,
  and which credential and address each push came from. Never a note, a project name or a
  PC's name.
- **What stays:** the git history keeps every encrypted file ever written. To start fully
  fresh, use a new repo.
- **The 12 words open everything:** with them and a copy of the repo, all of it can be read.
  Keep them in a password manager.

## Troubleshooting

| You see | What to do |
|---|---|
| "esa dirección lleva una contraseña o un token" | Use the address without it, with SSH or a credential helper |
| "git … falló: Permission denied / Authentication failed" | Check that `git ls-remote <address>` works in a terminal without a prompt |
| "Este repo es público" (the app's setup checks it; `hedy sync setup` does not) | Make it private: anyone could keep its encrypted history, see when you sync, and read it all if the 12 words ever leak |
| "ese repo ya tiene archivos" | Use a new empty repo, created without a README |
| "esta PC se quitó de la sincronización desde otra PC" | `hedy sync leave`, then join again with the 12 words |
| "otra PC acaba de cambiar las claves" | Try again: another PC was adding or removing one at the same time |
| A sync that never finishes | Hedy stops a git command after 15 minutes (30 s for a stalled HTTP transfer); check the network or the server |

## Testing it

- `cargo test`: the engine against an in-memory git server, including random multi-PC
  simulations. Every test also compacts.
- `scripts/sync-e2e.sh <empty-repo-url>`: the real `hedy` binary, four PCs on a real repo.
  It covers setup, join, wrong words, concurrent syncs, leaving and joining again, removing a
  PC and compaction. It uses the real keyring and leaves it clean.
- `HEDY_E2E_URL=<empty-repo-url> cargo test -p hedy-app -- --ignored sync_ipc`: the app's
  Tauri commands over IPC, with the arguments the UI sends. Two PCs, a conflict settled from
  the app, a project's memory home, removing a PC and leaving. Without `HEDY_E2E_URL` it uses
  a local bare repo.
