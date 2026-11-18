# Team memory

A team's shared memory: decisions, conventions and rules that several people's agents follow,
in a private git repo of the team's. Each note is encrypted and signed on the PC that wrote
it, and every PC checks, when it reads it, that its author was allowed to write it. The repo
only shows who the PCs are. The design, the threat model and every decision behind it are in
[decisions.md §35](decisions.md).

## Layers

| Layer | Who reads it | Who writes it |
|---|---|---|
| **Equipo** | Every agent in the team's projects, always, quoted with its author (at most 1,500 tokens) | Admins; a member's note becomes a suggestion an admin reviews |
| **Proyecto** | Agents in that project, when they search the memory | Anyone in the team, under their name |
| **Solo tú** | You | You (your personal memory, as before) |

Changing someone else's note is a **suggestion**: its owner (or an admin, for the team layer)
takes it or turns it down. A note or suggestion with hidden characters (text-direction
tricks, invisible characters) can never be accepted.

Your personal memory never goes to a team. A team's notes never go to your personal sync,
not even the team layer's.

## Create a team

1. Create an **empty private repo** (no README) on the team's git server: GitHub, GitLab,
   Bitbucket, Gitea or your own. Protect its `main` branch so nobody can force-push.
2. Make sure your git reaches it **without a password prompt** (an SSH key, or a credential
   helper). Hedy refuses an address with a token in it.
3. In the app: Ajustes → Equipos → **Crear un equipo**, or `hedy team create <address> --name
   <team>`.
4. Keep the team's **12 words** in the team's password manager. Without them, a team whose
   admins all lose their PCs is locked forever.

Then connect each project: its memory screen → **Memoria de este proyecto** → *Equipo ·
<team>*, or `hedy team link` in its folder. A project needs a git remote (`origin`): that is
how the same project is recognised on every PC.

## Invite someone

1. Give them **access to the repo on your git server**. Hedy cannot do that part.
2. In the team's card, **Crear invitación** (or `hedy team invite`, `--admin` for an admin).
   The code (`7-tigre-lima-nube-faro`) works **once, for 48 hours**.
3. Send them the code and the repo's address, through different channels if you can.
4. They join in the app (**Unirme con una invitación**) or with `hedy team join <address>`
   (the code is asked, never shown). They are in once your PC syncs, usually in under 5
   minutes.

The code is checked with a PAKE (SPAKE2): it never reaches the repo, and nobody with a copy
of the repo can test guesses offline. A wrong code burns the invite; make another.

A member adds another PC of their own with **Sumar otra PC mía** (`hedy team invite --mine`).
An admin's PC adds it on its next sync.

## People and roles

- **Admins** invite, remove, change roles and review the team layer. **Members** add or
  remove their own PCs, and write their project notes.
- The team keeps at least one admin: the last one cannot leave or step down. Make someone else
  an admin first.
- **Removing someone** (**Quitar**, `hedy team remove <name>`): the key changes, so their PCs
  read nothing written from then on. What they had stays on their PCs, and their notes stay
  under their name. **Also take away their access to the repo** on your git server.
- **Leaving** (**Salir del equipo**, `hedy team leave`): another admin's PC takes yours out on
  its next sync. `hedy team leave --here` (or **Olvidar en esta PC** while you wait) only
  forgets the team on this PC.
- **Cancelling a join** before you are in takes your request back, so the invite no longer
  works. If the PC that invited you let you in first, or you cancel without the network, the
  PC stays in the team until it is removed (by an admin, or by you for a PC of your own).

## Security in short

- **Encryption and signatures:** the same as personal sync (XChaCha20-Poly1305, Ed25519,
  epochs of keys). Each PC also has a seat: a person and a role.
- **Checked on read:** a member's PC cannot sign a new epoch, write the team layer, write in
  someone else's name, take over someone else's note, or move a note out of the team layer.
  What it writes against the rules is refused by every PC.
- **The agent reads team notes as data:** quoted, with their author, under a header that says
  they are not instructions.
- **New members read older notes:** each epoch carries the key before it, signed by the admin
  who wrote it. Someone removed never gets a new key.
- **What the git server sees:** the PCs' and people's random ids, their roles, how many PCs
  each person has, when invites were made and used, file sizes and times, and which git
  account pushed each change (which can tie a PC to a real person). Never a note, a name in
  Hedy or the team's name.
- **Limits:** someone who keeps push access after being removed can delete files, which
  stops syncs or cuts new members off from older notes. They cannot forge notes.

## Troubleshooting

| You see | What to do |
|---|---|
| "eso no es un código de invitación" | Check the number and the four words; a typo never burns the invite |
| "la invitación venció" / "alguien ya usó esa invitación" / "no funcionó" | Ask for another |
| Waiting to get in for a long time | The PC that invited you has to sync: ask them to open Hedy (or `hedy team now`). To give up, **Cancelar** (or `hedy team leave`) |
| "tu rol en el equipo no lo permite" | Ask an admin |
| "el equipo se quedaría sin admin" | Make someone else an admin first |
| "esta PC ya no está en el equipo" | An admin removed it: `hedy team leave`, and ask for another invite to come back |
| "la memoria de este proyecto no es del equipo" | Connect the project to the team first |
| "la nota tiene caracteres ocultos" | Write it again without them; the message shows where they were |

## Testing it

- `cargo test`: the team engine against an in-memory git server, including a random
  three-person simulation, and against real git and SQLite.
- `scripts/team-e2e.sh <empty-repo-url>`: the real `hedy` binary, three people (one with two
  PCs) on a real repo: invites, a burned code, notes and a suggestion, removing someone, and
  everyone leaving. It uses the real keyring and leaves it clean.
- `HEDY_E2E_URL=<empty-repo-url> cargo test -p hedy-app -- --ignored team_ipc`: the app's
  Tauri commands over IPC, with the arguments the UI sends. Without `HEDY_E2E_URL` it uses a
  local bare repo.
- End-to-end scenarios (`cargo test scenarios::local`), each PC a Hedy of its own on SQLite
  and real git:
  - **Personal memory:** three PCs sharing notes, templates and skills; the same note edited
    apart; a PC removed; a project kept on one PC; the 12 words bringing everything back.
  - **Teams:** a product team reviewing suggestions; people leaving, joining later and changing
    roles; one person with two PCs; invite codes; what a member forges; two admins leaving at
    once; an agent's proposal reaching the team.
  - **Both:** one person in personal sync and in two teams, and a project moving from personal
    memory to a team and back. Each note reaches only its own people.

  On a real server, one empty repo per scenario repo name (`p1-personal` … `m2-team`, see
  `Env::repo` in `scenarios.rs`): `HEDY_E2E_REPOS='https://host/me/e2e-{}.git' cargo test -p
  hedy-core -- --ignored scenarios::real`.
