//! Team memory (docs/decisions.md §35): a team is one more sync space, with
//! its own private repo, keys and replica, where each PC also belongs to a
//! person with a role.
//!
//! A team's notes live in its replica, never in this PC's notes (so nothing
//! of a team ever reaches personal sync). They change only through the use
//! cases here, which write a version to the replica and the outbox; the
//! next team sync sends it.
//!
//! Every PC checks what it reads ([`Hedy::team_allows`]): who wrote a
//! version, by their seat in the newest epoch, and whether their role lets
//! them. A note of the team layer that no admin wrote, or one in someone
//! else's name, is never taken.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

use super::*;
use crate::domain::memory::{Category, normalize_topic};
use crate::domain::team::{
    INVITE_TTL_SECS, PersonId, Role, Seat, SuggestionBody, TeamNoteBody, TeamWrite, hidden_chars,
    layout as team_layout, reveal_hidden,
};

/// The teams this PC is in (JSON list of ids). Each one's own settings are
/// its space's ([`Space::key`]).
const TEAMS: &str = "teams";
/// This PC's person in the team, and their name.
const PERSON: &str = "person";
pub(super) const PERSON_NAME: &str = "person_name";
/// The team's name, as an admin wrote it in the repo.
const TEAM_NAME: &str = "team_name";
/// A name this admin's PC gave the team, to write on its next sync.
const NEW_NAME: &str = "new_name";
/// The invites this PC made (JSON [`InviteRecord`] by number); their
/// secrets are in the keyring ([`InviteKeys`]).
const INVITES: &str = "invites";
/// Invites a wrong code burned (JSON list of numbers), until dismissed.
const FAILED: &str = "failed_invites";
/// While this PC joins (JSON [`Joining`]): set up only once it is in.
const JOIN: &str = "join";

/// The secret side of an invite this PC made, kept in the keyring.
#[derive(Serialize, Deserialize)]
pub(super) struct InviteKeys {
    code: String,
    /// SPAKE2's seed, until the first request burns it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secret: Option<String>,
    /// The key a request that opened shares, to seal the answer with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key: Option<String>,
}

/// An invite this PC made (not secret).
#[derive(Clone, Serialize, Deserialize)]
struct InviteRecord {
    /// For another PC of this PC's person; else for a new person.
    own_pc: bool,
    role: Role,
    expires: i64,
    /// The request that opened, until it is answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accepted: Option<JoinRequest>,
}

/// An invite's offer in the repo.
#[derive(Serialize, Deserialize)]
struct Offer {
    message: String,
    expires: i64,
}

/// A joiner's request in the repo: its SPAKE2 message, and a
/// [`JoinRequest`] sealed with the key they share (base64).
#[derive(Serialize, Deserialize)]
struct RequestFile {
    message: String,
    sealed: String,
}

/// What a joining PC asks for.
#[derive(Clone, Serialize, Deserialize)]
struct JoinRequest {
    device: DeviceId,
    public: String,
    device_name: String,
    person_name: String,
}

/// The inviting PC's answer, sealed with the invite's key: the root of
/// trust, so the joiner checks the team's epochs itself (a git host cannot
/// hand it a chain of its own).
#[derive(Serialize, Deserialize)]
struct JoinAnswer {
    recovery: String,
    pins: BTreeMap<u32, String>,
}

/// The key of the epoch before, as an epoch's history file holds it.
#[derive(Serialize, Deserialize)]
struct KeyHistory {
    epoch: u32,
    key: String,
}

/// What a member's PC asks an admin's PC to sign: changes to its own
/// person's PCs.
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SeatRequest {
    Add {
        device: DeviceId,
        public: String,
        person: PersonId,
    },
    Remove {
        device: DeviceId,
    },
}

/// A PC joining a team, until it is in.
#[derive(Serialize, Deserialize)]
struct Joining {
    url: String,
    number: u32,
}

/// An invite this PC made, as the app shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamInvite {
    pub number: u32,
    /// What the joiner types (`7-tigre-lima-nube-mar`).
    pub code: String,
    pub expires_at: i64,
    /// For another PC of this person (else a new person).
    pub own_pc: bool,
    pub role: Role,
    /// A request opened: the PC gets in on this PC's next syncs.
    pub answered: bool,
}

/// Where a join stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamJoin {
    pub id: String,
    /// In the team; else waiting for the inviting PC (or an admin's).
    pub joined: bool,
}

/// The team's name, as an admin's PC writes it in its folder (sealed and
/// signed): the newest one any admin wrote is the team's.
#[derive(Serialize, Deserialize)]
struct TeamInfo {
    name: String,
    /// When it was named (ms).
    at: i64,
}

/// A team this PC is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamSummary {
    pub id: String,
    pub name: String,
    pub url: String,
    /// Joining: waiting for the inviting PC to let this one in.
    pub waiting: bool,
    /// This PC's role; `None` when the newest epoch left it out.
    pub role: Option<Role>,
    pub last_sync: Option<i64>,
    pub last_error: Option<String>,
}

/// A person in a team, with their PCs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamPerson {
    pub person: PersonId,
    pub name: String,
    pub role: Role,
    pub devices: Vec<SyncDevice>,
    /// This PC's person.
    pub me: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamStatus {
    pub summary: TeamSummary,
    pub device: DeviceId,
    pub person: PersonId,
    /// Admins first, then by name.
    pub people: Vec<TeamPerson>,
    pub suggestions: usize,
    pub conflicts: usize,
    /// Invites this PC made, still open.
    pub invites: Vec<TeamInvite>,
    /// Invites a wrong code burned: someone has to ask for another.
    pub failed_invites: Vec<u32>,
}

/// A note of the team, as this PC has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamNote {
    pub uid: String,
    pub text: String,
    pub category: Category,
    pub topic: Option<String>,
    /// The whole team's (always in the agent's context), or a project's.
    pub team_layer: bool,
    /// A project's note: the project here with its remote, if any.
    pub project: Option<PathBuf>,
    pub person: PersonId,
    pub author: String,
    /// This PC's person wrote it.
    pub mine: bool,
    /// This PC's person may change it (else they suggest a change).
    pub can_edit: bool,
    pub created_at: i64,
    pub updated_at: Option<i64>,
}

/// A suggested change, as its reviewer sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamSuggestion {
    pub uid: String,
    /// The note it changes; `None`: a new note.
    pub note: Option<String>,
    /// The note's text now (`None`: a new note, or one gone since).
    pub before: Option<String>,
    /// The text it proposes, exactly as written (`None`: removing the
    /// note).
    pub after: Option<String>,
    /// `after` with every hidden character shown (`⟦U+202E⟧`): what a
    /// reviewer reads. Such a suggestion cannot be taken.
    pub shown: Option<String>,
    pub hidden: bool,
    pub category: Category,
    pub topic: Option<String>,
    pub team_layer: bool,
    pub project: Option<PathBuf>,
    pub person: PersonId,
    pub author: String,
    pub mine: bool,
    /// This PC's person may take it or turn it down.
    pub can_review: bool,
    pub created_at: i64,
}

/// What a team write became.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamWriteOutcome {
    /// The note itself, published with its byline.
    Published(String),
    /// A suggestion for whoever may change the note (its owner, an admin).
    Suggested(String),
}

/// This PC in a team, as its trusted epoch says.
struct Me {
    space: Space,
    device: DeviceId,
    seat: Seat,
    name: String,
}

fn note_body(version: &Version) -> Option<TeamNoteBody> {
    serde_json::from_str(version.body.as_deref()?).ok()
}

fn suggestion_body(version: &Version) -> Option<SuggestionBody> {
    serde_json::from_str(version.body.as_deref()?).ok()
}

/// The hidden characters in what a note shows or the agent reads.
fn note_is_clean(text: &str, author: &str, topic: Option<&str>) -> bool {
    hidden_chars(text).is_empty()
        && hidden_chars(author).is_empty()
        && topic.is_none_or(|t| hidden_chars(t).is_empty())
}

/// A team's repo README: the only plain file besides the key epochs.
fn team_readme() -> String {
    "# Hedy\n\nA team's shared memory in Hedy, sealed with XChaCha20-Poly1305 and signed on each \
     PC before it is pushed. Nothing here can be read without the team's keys.\n"
        .into()
}

impl Hedy {
    fn team_ids(&self) -> AppResult<Vec<String>> {
        Ok(self
            .store
            .get_setting(TEAMS)?
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default())
    }

    fn save_team_ids(&self, ids: &[String]) -> AppResult<()> {
        Ok(self.store.set_setting(TEAMS, Some(&json(&ids)))?)
    }

    /// The team named by `team` (its id, or its name), set up here.
    fn team_space(&self, team: &str) -> AppResult<Space> {
        let team = team.trim();
        let mut named = Vec::new();
        for id in self.team_ids()? {
            let space = Space::Team(id.clone());
            if !self.space_is_set_up(&space)? {
                continue;
            }
            if id == team {
                return Ok(space);
            }
            if self
                .sync_setting(&space, TEAM_NAME)?
                .as_deref()
                .map(str::trim)
                == Some(team)
            {
                named.push(space);
            }
        }
        match named.len() {
            1 => Ok(named.remove(0)),
            _ => Err(AppError::TeamNotFound(team.to_owned())),
        }
    }

    /// Like [`Self::team_space`], also for a team this PC is joining.
    fn team_space_or_joining(&self, team: &str) -> AppResult<Space> {
        match self.team_space(team) {
            Err(AppError::TeamNotFound(_)) if self.team_ids()?.iter().any(|id| id == team) => {
                let space = Space::Team(team.to_owned());
                match self.sync_json::<Joining>(&space, JOIN)? {
                    Some(_) => Ok(space),
                    None => Err(AppError::TeamNotFound(team.to_owned())),
                }
            }
            other => other,
        }
    }

    /// This PC's seat in the team's trusted epoch.
    fn team_me(&self, space: &Space) -> AppResult<Me> {
        let (status, members) = self.space_status(space)?.ok_or(AppError::SyncNotSetUp)?;
        let seat = members
            .into_iter()
            .find(|m| m.device.this_pc)
            .and_then(|m| m.seat)
            .ok_or(AppError::SyncRemoved)?;
        Ok(Me {
            space: space.clone(),
            device: status.device,
            seat,
            name: self.sync_setting(space, PERSON_NAME)?.unwrap_or_default(),
        })
    }

    /// Starts a team in the empty repo at `url`, with this PC as its first
    /// admin. Returns the team's id and its 12 recovery words, to keep in
    /// the team's password manager.
    pub fn team_create(
        &self,
        url: &str,
        team_name: &str,
        person_name: &str,
        device_name: &str,
    ) -> AppResult<(String, String)> {
        let id = self.sync_crypto.random_hex(8)?;
        let space = Space::Team(id.clone());
        let _guard = self.sync_guard(&space)?;
        let words = self.starting_sync(&space, url, || {
            self.create_team_locked(&space, url, team_name, person_name, device_name)
        })?;
        Ok((id, words))
    }

    fn create_team_locked(
        &self,
        space: &Space,
        url: &str,
        team_name: &str,
        person_name: &str,
        device_name: &str,
    ) -> AppResult<String> {
        let Space::Team(id) = space else {
            unreachable!("a team's space");
        };
        let dir = self.sync_dir(space)?;
        self.sync_remote.open(url, &dir)?;
        if !self.epochs(&dir)?.is_empty() {
            return Err(AppError::SyncRepoInUse);
        }
        if !self.sync_remote.files(&dir, "")?.is_empty() {
            return Err(AppError::SyncRepoNotEmpty);
        }
        let device = DeviceId(self.sync_crypto.random_hex(8)?);
        let person = PersonId(self.sync_crypto.random_hex(8)?);
        let me = self.sync_crypto.new_member()?;
        let (words, recovery) = self.sync_crypto.new_recovery()?;
        let recovery_public = self.sync_crypto.member_public(&recovery)?;
        let members = [
            EpochMember::new(RECOVERY_MEMBER, recovery_public.clone()),
            EpochMember {
                id: device.as_str().to_owned(),
                public: self.sync_crypto.member_public(&me)?,
                seat: Some(Seat {
                    person: person.clone(),
                    role: Role::Admin,
                }),
            },
        ];
        let (file, key) =
            self.sync_crypto
                .new_epoch(1, true, &members, RECOVERY_MEMBER, &recovery)?;
        self.sync_remote
            .write(&dir, &layout::epoch(1), file.as_bytes())?;
        self.sync_remote
            .write(&dir, layout::README, team_readme().as_bytes())?;
        let keys = Keys {
            device: me,
            epochs: BTreeMap::from([(1, key)]),
            ..Keys::default()
        };
        self.publish_epoch(space, &dir, &keys)?;
        let mut ids = self.team_ids()?;
        if !ids.contains(id) {
            ids.push(id.clone());
            self.save_team_ids(&ids)?;
        }
        self.set_sync_setting(space, PERSON, person.as_str())?;
        self.set_sync_setting(space, PERSON_NAME, person_name.trim())?;
        self.set_sync_setting(space, TEAM_NAME, team_name.trim())?;
        self.set_sync_setting(space, NEW_NAME, team_name.trim())?;
        let pins = BTreeMap::from([(1, self.sync_crypto.digest(file.as_bytes()))]);
        self.remember_sync(space, url, &device, device_name, &recovery_public, 1, &pins)?;
        // Set up: the words must reach the user even if this sync fails.
        let _ = self.sync_locked(space);
        Ok(words)
    }

    /// The teams this PC is in.
    pub fn teams(&self) -> AppResult<Vec<TeamSummary>> {
        let mut teams = Vec::new();
        for id in self.team_ids()? {
            let space = Space::Team(id.clone());
            if let Some(status) = self.team_summary(&space)? {
                teams.push(status);
            } else if let Some(joining) = self.sync_json::<Joining>(&space, JOIN)? {
                teams.push(TeamSummary {
                    id,
                    name: String::new(),
                    url: joining.url,
                    waiting: true,
                    role: None,
                    last_sync: None,
                    last_error: self.sync_setting(&space, LAST_ERROR)?,
                });
            }
        }
        Ok(teams)
    }

    fn team_summary(&self, space: &Space) -> AppResult<Option<TeamSummary>> {
        let Space::Team(id) = space else {
            return Ok(None);
        };
        let Some((status, members)) = self.space_status(space)? else {
            return Ok(None);
        };
        let role = members
            .iter()
            .find(|m| m.device.this_pc)
            .and_then(|m| m.seat.as_ref())
            .map(|s| s.role);
        Ok(Some(TeamSummary {
            id: id.clone(),
            name: self.sync_setting(space, TEAM_NAME)?.unwrap_or_default(),
            url: status.url,
            waiting: false,
            role,
            last_sync: status.last_sync,
            last_error: status.last_error,
        }))
    }

    /// The team's state and its people, from the clone only.
    pub fn team_status(&self, team: &str) -> AppResult<TeamStatus> {
        let space = self.team_space(team)?;
        let summary = self
            .team_summary(&space)?
            .ok_or_else(|| AppError::TeamNotFound(team.to_owned()))?;
        let (status, members) = self
            .space_status(&space)?
            .ok_or_else(|| AppError::TeamNotFound(team.to_owned()))?;
        let me = PersonId(self.sync_setting(&space, PERSON)?.unwrap_or_default());
        let mut people: BTreeMap<PersonId, TeamPerson> = BTreeMap::new();
        for member in members {
            let Some(seat) = member.seat else { continue };
            let person = people
                .entry(seat.person.clone())
                .or_insert_with(|| TeamPerson {
                    me: seat.person == me,
                    person: seat.person.clone(),
                    name: String::new(),
                    role: seat.role,
                    devices: Vec::new(),
                });
            // The name the PC that synced last wrote.
            let newest = person.devices.iter().filter_map(|d| d.last_sync).max();
            if let Some(name) = member.person_name
                && (person.name.is_empty() || member.device.last_sync > newest)
            {
                person.name = name;
            }
            person.devices.push(member.device);
        }
        let mut people: Vec<TeamPerson> = people.into_values().collect();
        people.sort_by(|a, b| {
            (a.role != Role::Admin, a.name.to_lowercase())
                .cmp(&(b.role != Role::Admin, b.name.to_lowercase()))
        });
        let suggestions = self.team_suggestions(team)?.len();
        Ok(TeamStatus {
            summary,
            device: status.device,
            person: me,
            people,
            suggestions,
            conflicts: status.conflicts,
            invites: self.team_invites(&space)?,
            failed_invites: self.sync_json(&space, FAILED)?.unwrap_or_default(),
        })
    }

    /// One sync of the team, now: for a PC still joining, a look at
    /// whether it is in.
    pub fn team_sync(&self, team: &str) -> AppResult<SyncReport> {
        let space = self.team_space_or_joining(team)?;
        let _guard = self.sync_guard(&space)?;
        let result = if self.space_is_set_up(&space)? {
            self.sync_locked(&space)
                .and_then(|report| self.team_seats(&space).map(|()| report))
        } else {
            self.finish_join(&space).map(|_| SyncReport::default())
        };
        match &result {
            Ok(_) => self.clear_sync_setting(&space, LAST_ERROR)?,
            Err(error) => self.set_sync_setting(&space, LAST_ERROR, &shown_error(error))?,
        }
        result
    }

    /// Whether a version another PC of the team wrote may be taken, by
    /// that PC's seat in the newest epoch (docs/decisions.md §35): checked
    /// by every PC that reads it.
    pub(super) fn team_allows(
        &self,
        session: &Session,
        change: &Change,
        known: Option<&Version>,
    ) -> AppResult<bool> {
        let Some(writer) = session.seats.get(change.version.stamp.device.as_str()) else {
            return Ok(false);
        };
        let team_layer = change.scope == Scope::Global;
        Ok(match change.kind {
            ItemKind::Note => {
                let owner = match &change.version.body {
                    None => None,
                    Some(_) => match note_body(&change.version) {
                        Some(note)
                            if note.person.is_valid()
                                && note_is_clean(
                                    &note.text,
                                    &note.author,
                                    note.topic.as_deref(),
                                ) =>
                        {
                            Some(note.person)
                        }
                        // Unreadable, or with hidden characters: never taken.
                        _ => return Ok(false),
                    },
                };
                let replaced = known.and_then(note_body).map(|n| n.person);
                TeamWrite {
                    writer,
                    team_layer,
                    owner: owner.as_ref(),
                    replaced_owner: replaced.as_ref(),
                    note_owner: None,
                }
                .note_allowed()
            }
            ItemKind::Suggestion => {
                let new = match &change.version.body {
                    None => None,
                    Some(_) => match suggestion_body(&change.version) {
                        Some(s) if s.person.is_valid() => Some(s),
                        _ => return Ok(false),
                    },
                };
                let old = known.and_then(suggestion_body);
                let note = new.as_ref().or(old.as_ref()).and_then(|s| s.note.clone());
                let note_owner = match note {
                    Some(uid) => self
                        .store
                        .sync_item(&session.space.id(), ItemKind::Note, &uid)?
                        .and_then(|(_, v)| note_body(&v))
                        .map(|n| n.person),
                    None => None,
                };
                TeamWrite {
                    writer,
                    team_layer,
                    owner: new.as_ref().map(|s| &s.person),
                    replaced_owner: old.as_ref().map(|s| &s.person),
                    note_owner: note_owner.as_ref(),
                }
                .suggestion_allowed()
            }
            // A team holds notes and suggestions only.
            _ => false,
        })
    }

    /// What a team sync does after applying the other PCs' versions: an
    /// admin's PC keeps the team's name in the repo; every PC keeps the
    /// name it reads there.
    pub(super) fn team_duties(&self, session: &Session) -> AppResult<()> {
        let space = &session.space;
        let admin = session
            .seats
            .get(session.device.as_str())
            .is_some_and(|s| s.role == Role::Admin);
        let renamed = self.sync_setting(space, NEW_NAME)?;
        match (self.read_team_info(session), renamed) {
            (_, Some(name)) if admin => {
                let info = TeamInfo {
                    name: name.clone(),
                    at: self.clock.now_ms(),
                };
                self.write_signed(
                    session,
                    &team_layout::team_info(&session.device),
                    &json(&info),
                )?;
                self.set_sync_setting(space, TEAM_NAME, &name)?;
                self.clear_sync_setting(space, NEW_NAME)?;
            }
            (Some(info), _) => self.set_sync_setting(space, TEAM_NAME, &info.name)?,
            _ => {}
        }
        self.open_requests(session)
    }

    /// The team's name in the repo: the newest that an admin's PC of the
    /// newest epoch signed.
    fn read_team_info(&self, session: &Session) -> Option<TeamInfo> {
        session
            .seats
            .iter()
            .filter(|(_, seat)| seat.role == Role::Admin)
            .filter_map(|(device, _)| {
                let device = DeviceId(device.clone());
                let body = self
                    .open_signed(session, &device, &team_layout::team_info(&device))
                    .ok()
                    .flatten()?;
                serde_json::from_str::<TeamInfo>(&body).ok()
            })
            .max_by(|a, b| (a.at, &a.name).cmp(&(b.at, &b.name)))
    }

    /// Writes a version of a team item here (the replica) and leaves it for
    /// the next team sync (the outbox).
    fn team_write(
        &self,
        me: &Me,
        kind: ItemKind,
        uid: &str,
        scope: Scope,
        body: Option<String>,
    ) -> AppResult<()> {
        let space = &me.space;
        let known = self.store.sync_item(&space.id(), kind, uid)?;
        let stamp = self.next_stamp(space, &me.device)?;
        let version = match &known {
            Some((_, known)) => known.replaced(stamp, body),
            None => Version::new(stamp, body),
        };
        let change = Change {
            kind,
            uid: uid.to_owned(),
            scope,
            version,
        };
        self.store.put_sync_item(&space.id(), &change)?;
        let mut outbox: Vec<Change> = self.sync_json(space, OUTBOX)?.unwrap_or_default();
        outbox.push(change);
        self.set_sync_setting(space, OUTBOX, &json(&outbox))
    }

    /// The team's sealed id of the project in `path`, when its memory is
    /// the team's.
    fn team_scope(&self, space: &Space, project: Option<&Path>) -> AppResult<Scope> {
        let Some(project) = project else {
            return Ok(Scope::Global);
        };
        let project = self.project_at(project)?;
        let name_key = self.team_name_key(space)?;
        let SyncedProjects { project_ids, .. } = self.synced_projects(space, &name_key)?;
        project_ids
            .get(&project.path)
            .map(|id| Scope::Project(id.clone()))
            .ok_or(AppError::TeamProjectNotLinked)
    }

    /// What names projects in the team's repo: its first epoch's key, the
    /// same on every PC.
    fn team_name_key(&self, space: &Space) -> AppResult<String> {
        self.keys(space)?
            .epochs
            .get(&1)
            .cloned()
            .ok_or(AppError::SyncRemoved)
    }

    /// Connects the project in `path` to the team: its notes for the team
    /// go to the team's repo (and never to personal sync).
    pub fn team_link_project(&self, team: &str, project: &Path) -> AppResult<()> {
        let Space::Team(id) = self.team_space(team)? else {
            unreachable!("a team's space");
        };
        self.set_memory_home(project, Some(&MemoryHome::Team(id)))
    }

    /// Adds a note to the team: to a project of the team, or (`None`) to
    /// the team layer. A member's note for the team layer becomes a
    /// suggestion the admins review.
    pub fn team_add_note(
        &self,
        team: &str,
        project: Option<&Path>,
        text: &str,
        category: Category,
        topic: Option<&str>,
    ) -> AppResult<TeamWriteOutcome> {
        let space = self.team_space(team)?;
        let _guard = self.sync_guard(&space)?;
        let me = self.team_me(&space)?;
        let text = text.trim();
        let topic = topic.map(normalize_topic).transpose()?.flatten();
        check_clean(text)?;
        let scope = self.team_scope(&space, project)?;
        let now = self.now_secs();
        let uid = self.sync_crypto.random_hex(16)?;
        if scope == Scope::Global && me.seat.role != Role::Admin {
            let suggestion = SuggestionBody {
                note: None,
                text: Some(text.to_owned()),
                category,
                topic,
                person: me.seat.person.clone(),
                author: me.name.clone(),
                created_at: now,
            };
            self.team_write(
                &me,
                ItemKind::Suggestion,
                &uid,
                scope,
                Some(json(&suggestion)),
            )?;
            return Ok(TeamWriteOutcome::Suggested(uid));
        }
        let note = TeamNoteBody {
            text: text.to_owned(),
            category,
            person: me.seat.person.clone(),
            author: me.name.clone(),
            created_at: now,
            topic,
            updated_at: None,
        };
        self.team_write(&me, ItemKind::Note, &uid, scope, Some(json(&note)))?;
        Ok(TeamWriteOutcome::Published(uid))
    }

    /// A note of the team as it is now; `None` when gone.
    fn team_note(&self, space: &Space, uid: &str) -> AppResult<Option<(Scope, TeamNoteBody)>> {
        Ok(self
            .store
            .sync_item(&space.id(), ItemKind::Note, uid)?
            .and_then(|(scope, v)| Some((scope, note_body(&v)?))))
    }

    /// Whether this PC's person may change a note (else they suggest).
    fn may_edit(me: &Me, scope: &Scope, note: &TeamNoteBody) -> bool {
        TeamWrite {
            writer: &me.seat,
            team_layer: *scope == Scope::Global,
            owner: Some(&note.person),
            replaced_owner: Some(&note.person),
            note_owner: None,
        }
        .note_allowed()
    }

    /// Changes a note of the team (`text: None` removes it). Someone else's
    /// note, or one of the team layer for a member, gets a suggestion
    /// instead.
    pub fn team_edit_note(
        &self,
        team: &str,
        uid: &str,
        text: Option<&str>,
        category: Option<Category>,
        topic: Option<&str>,
    ) -> AppResult<TeamWriteOutcome> {
        let space = self.team_space(team)?;
        let _guard = self.sync_guard(&space)?;
        let me = self.team_me(&space)?;
        let (scope, note) = self
            .team_note(&space, uid)?
            .ok_or_else(|| AppError::TeamItemNotFound(uid.to_owned()))?;
        let text = text.map(str::trim);
        if let Some(text) = text {
            check_clean(text)?;
        }
        let category = category.unwrap_or(note.category);
        let topic = match topic {
            Some(topic) => normalize_topic(topic)?,
            None => note.topic.clone(),
        };
        if !Self::may_edit(&me, &scope, &note) {
            let suggestion = SuggestionBody {
                note: Some(uid.to_owned()),
                text: text.map(str::to_owned),
                category,
                topic,
                person: me.seat.person.clone(),
                author: me.name.clone(),
                created_at: self.now_secs(),
            };
            let sid = self.sync_crypto.random_hex(16)?;
            self.team_write(
                &me,
                ItemKind::Suggestion,
                &sid,
                scope,
                Some(json(&suggestion)),
            )?;
            return Ok(TeamWriteOutcome::Suggested(sid));
        }
        let body = text.map(|text| {
            json(&TeamNoteBody {
                text: text.to_owned(),
                category,
                topic,
                updated_at: Some(self.now_secs()),
                ..note
            })
        });
        self.team_write(&me, ItemKind::Note, uid, scope, body)?;
        Ok(TeamWriteOutcome::Published(uid.to_owned()))
    }

    /// The team's notes this PC has.
    pub fn team_notes(&self, team: &str) -> AppResult<Vec<TeamNote>> {
        let space = self.team_space(team)?;
        let me = self.team_me(&space)?;
        let projects = self.team_projects(&space)?;
        let mut notes: Vec<TeamNote> = self
            .store
            .sync_items(&space.id())?
            .into_iter()
            .filter(|c| c.kind == ItemKind::Note)
            .filter_map(|c| {
                let note = note_body(&c.version)?;
                let can_edit = Self::may_edit(&me, &c.scope, &note);
                Some(TeamNote {
                    uid: c.uid,
                    team_layer: c.scope == Scope::Global,
                    project: project_here(&projects, &c.scope),
                    mine: note.person == me.seat.person,
                    can_edit,
                    text: note.text,
                    category: note.category,
                    topic: note.topic,
                    person: note.person,
                    author: note.author,
                    created_at: note.created_at,
                    updated_at: note.updated_at,
                })
            })
            .collect();
        notes.sort_by_key(|n| (!n.team_layer, n.created_at));
        Ok(notes)
    }

    /// Sealed project id → the project here, for the team's projects.
    fn team_projects(&self, space: &Space) -> AppResult<HashMap<String, Vec<PathBuf>>> {
        let name_key = self.team_name_key(space)?;
        Ok(self.synced_projects(space, &name_key)?.projects)
    }

    /// The suggestions waiting in the team.
    pub fn team_suggestions(&self, team: &str) -> AppResult<Vec<TeamSuggestion>> {
        let space = self.team_space(team)?;
        let me = self.team_me(&space)?;
        let projects = self.team_projects(&space)?;
        let mut out = Vec::new();
        for change in self.store.sync_items(&space.id())? {
            if change.kind != ItemKind::Suggestion {
                continue;
            }
            let Some(s) = suggestion_body(&change.version) else {
                continue;
            };
            let note = match &s.note {
                Some(uid) => self.team_note(&space, uid)?,
                None => None,
            };
            let can_review = Self::may_review(&me, &change.scope, note.as_ref().map(|n| &n.1));
            out.push(TeamSuggestion {
                uid: change.uid,
                before: note.map(|n| n.1.text),
                shown: s.text.as_deref().map(reveal_hidden),
                hidden: s
                    .text
                    .as_deref()
                    .is_some_and(|t| !hidden_chars(t).is_empty()),
                mine: s.person == me.seat.person,
                team_layer: change.scope == Scope::Global,
                project: project_here(&projects, &change.scope),
                note: s.note,
                after: s.text,
                category: s.category,
                topic: s.topic,
                person: s.person,
                author: s.author,
                can_review,
                created_at: s.created_at,
            });
        }
        out.sort_by_key(|s| s.created_at);
        Ok(out)
    }

    /// Whether this PC's person may take a suggestion: an admin, or the
    /// owner of the project note it changes.
    fn may_review(me: &Me, scope: &Scope, note: Option<&TeamNoteBody>) -> bool {
        me.seat.role == Role::Admin
            || (*scope != Scope::Global && note.is_some_and(|n| n.person == me.seat.person))
    }

    fn team_suggestion(&self, space: &Space, uid: &str) -> AppResult<(Scope, SuggestionBody)> {
        self.store
            .sync_item(&space.id(), ItemKind::Suggestion, uid)?
            .and_then(|(scope, v)| Some((scope, suggestion_body(&v)?)))
            .ok_or_else(|| AppError::TeamItemNotFound(uid.to_owned()))
    }

    /// Takes a suggestion: the note changes (or is added, or removed) as it
    /// says, keeping its byline, and the suggestion goes.
    pub fn team_accept_suggestion(&self, team: &str, uid: &str) -> AppResult<()> {
        let space = self.team_space(team)?;
        let _guard = self.sync_guard(&space)?;
        let me = self.team_me(&space)?;
        let (scope, s) = self.team_suggestion(&space, uid)?;
        let note = match &s.note {
            Some(note) => self.team_note(&space, note)?,
            None => None,
        };
        if !Self::may_review(&me, &scope, note.as_ref().map(|n| &n.1)) {
            return Err(AppError::TeamNotAllowed);
        }
        if let Some(text) = &s.text {
            check_clean(text)?;
        }
        let now = self.now_secs();
        match (&s.note, note, s.text) {
            // A new note, in its author's name.
            (None, _, Some(text)) => {
                let body = TeamNoteBody {
                    text,
                    category: s.category,
                    person: s.person,
                    author: s.author,
                    created_at: now,
                    topic: s.topic,
                    updated_at: None,
                };
                let note_uid = self.sync_crypto.random_hex(16)?;
                self.team_write(
                    &me,
                    ItemKind::Note,
                    &note_uid,
                    scope.clone(),
                    Some(json(&body)),
                )?;
            }
            (Some(note_uid), Some((note_scope, note)), text) => {
                let body = text.map(|text| {
                    json(&TeamNoteBody {
                        text,
                        category: s.category,
                        topic: s.topic,
                        updated_at: Some(now),
                        ..note
                    })
                });
                self.team_write(&me, ItemKind::Note, note_uid, note_scope, body)?;
            }
            // Its note is gone, or it removes nothing: it only goes.
            _ => {}
        }
        self.team_write(&me, ItemKind::Suggestion, uid, scope, None)
    }

    /// Turns a suggestion down (or, its author, takes it back).
    pub fn team_reject_suggestion(&self, team: &str, uid: &str) -> AppResult<()> {
        let space = self.team_space(team)?;
        let _guard = self.sync_guard(&space)?;
        let me = self.team_me(&space)?;
        let (scope, s) = self.team_suggestion(&space, uid)?;
        let note = match &s.note {
            Some(note) => self.team_note(&space, note)?,
            None => None,
        };
        if s.person != me.seat.person && !Self::may_review(&me, &scope, note.as_ref().map(|n| &n.1))
        {
            return Err(AppError::TeamNotAllowed);
        }
        self.team_write(&me, ItemKind::Suggestion, uid, scope, None)
    }

    // ── Invites (docs/decisions.md §35) ─────────────────────────────────

    /// What both PCs of an invite bind into SPAKE2: the team's first epoch,
    /// so an invite answers for this team only.
    fn invite_context(&self, dir: &Path) -> AppResult<String> {
        let epochs = self.epochs(dir)?;
        match epochs.first() {
            Some((1, file)) => Ok(self.sync_crypto.digest(file.as_bytes())),
            _ => Err(AppError::SyncRepoEmpty),
        }
    }

    fn invite_records(&self, space: &Space) -> AppResult<BTreeMap<u32, InviteRecord>> {
        Ok(self.sync_json(space, INVITES)?.unwrap_or_default())
    }

    fn save_invite_records(
        &self,
        space: &Space,
        records: &BTreeMap<u32, InviteRecord>,
    ) -> AppResult<()> {
        self.set_sync_setting(space, INVITES, &json(records))
    }

    /// Invites a new person to the team (admins only), with `role`. The code
    /// works once, for 48 hours; the repo never holds it.
    pub fn team_invite(&self, team: &str, role: Role) -> AppResult<TeamInvite> {
        self.make_invite(team, false, role)
    }

    /// Invites another PC of this PC's person (any member): it joins with
    /// the same role.
    pub fn team_invite_pc(&self, team: &str) -> AppResult<TeamInvite> {
        self.make_invite(team, true, Role::Member)
    }

    fn make_invite(&self, team: &str, own_pc: bool, role: Role) -> AppResult<TeamInvite> {
        let space = self.team_space(team)?;
        let _guard = self.sync_guard(&space)?;
        let me = self.team_me(&space)?;
        if !own_pc && me.seat.role != Role::Admin {
            return Err(AppError::TeamNotAllowed);
        }
        let role = if own_pc { me.seat.role } else { role };
        let url = self
            .sync_setting(&space, URL)?
            .ok_or(AppError::SyncNotSetUp)?;
        let dir = self.sync_dir(&space)?;
        self.sync_remote.open(&url, &dir)?;
        let context = self.invite_context(&dir)?;
        let used: std::collections::HashSet<u32> = self
            .sync_remote
            .files(&dir, team_layout::INVITES_DIR)?
            .iter()
            .filter_map(|p| team_layout::invite_number(p))
            .collect();
        let mut number = 0;
        for _ in 0..64 {
            let random = u16::from_str_radix(&self.sync_crypto.random_hex(2)?, 16).unwrap_or(0);
            let candidate = 1 + u32::from(random) % 999;
            if !used.contains(&candidate) {
                number = candidate;
                break;
            }
        }
        if number == 0 {
            return Err(AppError::SyncBusy);
        }
        let code = self.sync_crypto.invite_code(number)?;
        let (secret, message) = self.sync_crypto.invite_start(&code, &context)?;
        let expires = self.now_secs() + INVITE_TTL_SECS;
        self.sync_remote.write(
            &dir,
            &team_layout::offer(number),
            json(&Offer { message, expires }).as_bytes(),
        )?;
        // Kept before the offer leaves, so an offer out there always has
        // its secret here.
        let mut keys = self.keys(&space)?;
        keys.invites.insert(
            number,
            InviteKeys {
                code: code.clone(),
                secret: Some(secret),
                key: None,
            },
        );
        self.save_keys(&space, &keys)?;
        let mut records = self.invite_records(&space)?;
        let record = InviteRecord {
            own_pc,
            role,
            expires,
            accepted: None,
        };
        records.insert(number, record);
        self.save_invite_records(&space, &records)?;
        // Another PC took the same number meanwhile: start over.
        if let Err(error) = self.sync_remote.push(&dir, "hedy team") {
            let _ = self.sync_remote.discard_local(&dir);
            self.forget_invite(&space, number)?;
            return Err(error.into());
        }
        Ok(TeamInvite {
            number,
            code,
            expires_at: expires,
            own_pc,
            role,
            answered: false,
        })
    }

    /// Forgets an invite this PC made (its secrets and its record).
    fn forget_invite(&self, space: &Space, number: u32) -> AppResult<()> {
        let mut keys = self.keys(space)?;
        if keys.invites.remove(&number).is_some() {
            self.save_keys(space, &keys)?;
        }
        let mut records = self.invite_records(space)?;
        if records.remove(&number).is_some() {
            self.save_invite_records(space, &records)?;
        }
        Ok(())
    }

    /// The invites this PC made that are still open.
    fn team_invites(&self, space: &Space) -> AppResult<Vec<TeamInvite>> {
        let keys = self.keys(space)?;
        Ok(self
            .invite_records(space)?
            .into_iter()
            .filter_map(|(number, record)| {
                Some(TeamInvite {
                    number,
                    code: keys.invites.get(&number)?.code.clone(),
                    expires_at: record.expires,
                    own_pc: record.own_pc,
                    role: record.role,
                    answered: record.accepted.is_some(),
                })
            })
            .collect())
    }

    /// Takes back an invite not used yet.
    pub fn team_cancel_invite(&self, team: &str, number: u32) -> AppResult<()> {
        let space = self.team_space(team)?;
        let _guard = self.sync_guard(&space)?;
        if !self.invite_records(&space)?.contains_key(&number) {
            return Err(AppError::TeamInviteNotFound(number));
        }
        let dir = self.sync_dir(&space)?;
        for path in [team_layout::offer(number), team_layout::request(number)] {
            self.sync_remote.remove(&dir, &path)?;
        }
        self.forget_invite(&space, number)?;
        Ok(self.sync_remote.push(&dir, "hedy team")?)
    }

    /// Forgets that an invite was burned (the user saw it).
    pub fn team_dismiss_failed_invite(&self, team: &str, number: u32) -> AppResult<()> {
        let space = self.team_space(team)?;
        let failed: Vec<u32> = self
            .sync_json::<Vec<u32>>(&space, FAILED)?
            .unwrap_or_default()
            .into_iter()
            .filter(|n| *n != number)
            .collect();
        self.set_sync_setting(&space, FAILED, &json(&failed))
    }

    /// Opens the requests that answer this PC's invites (in a sync, before
    /// its push). Each request is one guess at the code: the invite burns
    /// before it is checked, and one that does not open stays burned. An
    /// expired invite goes.
    fn open_requests(&self, session: &Session) -> AppResult<()> {
        let space = &session.space;
        let mut records = self.invite_records(space)?;
        if records.is_empty() {
            return Ok(());
        }
        let now = self.now_secs();
        let context = self.invite_context(&session.dir)?;
        for (number, record) in records.clone() {
            if record.accepted.is_some() {
                continue;
            }
            let offer = self.read_repo(&session.dir, &team_layout::offer(number))?;
            if record.expires < now || offer.is_none() {
                self.drop_invite_files(session, number)?;
                self.forget_invite(space, number)?;
                records.remove(&number);
                continue;
            }
            let Some(request) = self.read_repo(&session.dir, &team_layout::request(number))? else {
                continue;
            };
            // Burn first: a crash after this leaves no second guess.
            let mut keys = self.keys(space)?;
            let Some(invite) = keys.invites.get_mut(&number) else {
                continue;
            };
            let secret = invite.secret.take();
            let code = invite.code.clone();
            self.save_keys(space, &keys)?;
            let opened = secret.and_then(|secret| {
                let file: RequestFile = serde_json::from_slice(&request).ok()?;
                let key = self
                    .sync_crypto
                    .invite_accept(&secret, &code, &context, &file.message)
                    .ok()?;
                let sealed = BASE64.decode(file.sealed).ok()?;
                let plain = self
                    .sync_crypto
                    .invite_open(&key, &team_layout::request(number), &sealed)
                    .ok()?;
                let request: JoinRequest = serde_json::from_slice(&plain).ok()?;
                (request.device.is_valid()
                    && !session.members.contains_key(request.device.as_str()))
                .then_some((key, request))
            });
            self.drop_invite_files(session, number)?;
            match opened {
                Some((key, request)) => {
                    let mut keys = self.keys(space)?;
                    if let Some(invite) = keys.invites.get_mut(&number) {
                        invite.key = Some(key);
                    }
                    self.save_keys(space, &keys)?;
                    records.insert(
                        number,
                        InviteRecord {
                            accepted: Some(request),
                            ..record
                        },
                    );
                }
                None => {
                    self.forget_invite(space, number)?;
                    records.remove(&number);
                    let mut failed: Vec<u32> = self.sync_json(space, FAILED)?.unwrap_or_default();
                    failed.push(number);
                    self.set_sync_setting(space, FAILED, &json(&failed))?;
                }
            }
            self.save_invite_records(space, &records)?;
        }
        self.save_invite_records(space, &records)
    }

    fn drop_invite_files(&self, session: &Session, number: u32) -> AppResult<()> {
        for path in [team_layout::offer(number), team_layout::request(number)] {
            if self.read_repo(&session.dir, &path)?.is_some() {
                self.sync_remote.remove(&session.dir, &path)?;
            }
        }
        Ok(())
    }

    /// After a sync: the seat changes this PC makes, in a commit of their
    /// own (another admin's PC may write the same epoch first; then they
    /// are made again on the next sync).
    /// - An admin's PC adds the PCs whose invites it answered, and signs
    ///   what members' PCs asked about their own PCs.
    /// - A member's PC asks an admin's to add the PC it invited.
    ///
    /// Then each answered invite gets its answer.
    fn team_seats(&self, space: &Space) -> AppResult<()> {
        let records = self.invite_records(space)?;
        let accepted: Vec<(u32, JoinRequest)> = records
            .iter()
            .filter_map(|(n, r)| Some((*n, r.accepted.clone()?)))
            .collect();
        let me = self.team_me(space)?;
        let dir = self.sync_dir(space)?;
        let session = self.sync_session(space, dir.clone(), me.device.clone())?;
        let admin = me.seat.role == Role::Admin;
        let mut added: Vec<EpochMember> = Vec::new();
        let mut removed: Vec<String> = Vec::new();
        let mut done: Vec<String> = Vec::new();
        if admin {
            for (_, request) in &accepted {
                let own = records.values().any(|r| {
                    r.own_pc
                        && r.accepted
                            .as_ref()
                            .is_some_and(|a| a.device == request.device)
                });
                let (person, role) = if own {
                    (me.seat.person.clone(), me.seat.role)
                } else {
                    let role = records
                        .values()
                        .find(|r| {
                            r.accepted
                                .as_ref()
                                .is_some_and(|a| a.device == request.device)
                        })
                        .map_or(Role::Member, |r| r.role);
                    (PersonId(self.sync_crypto.random_hex(8)?), role)
                };
                added.push(EpochMember {
                    id: request.device.as_str().to_owned(),
                    public: request.public.clone(),
                    seat: Some(Seat { person, role }),
                });
            }
            for path in self.sync_remote.files(&dir, team_layout::REQUESTS_DIR)? {
                done.push(path.clone());
                let Some((signer, _)) = team_layout::seat_request_of(&path) else {
                    continue;
                };
                let Some(seat) = session.seats.get(signer.as_str()) else {
                    continue;
                };
                let Ok(Some(body)) = self.open_signed(&session, &signer, &path) else {
                    continue;
                };
                match serde_json::from_str::<SeatRequest>(&body) {
                    Ok(SeatRequest::Add {
                        device,
                        public,
                        person,
                    }) if person == seat.person
                        && device.is_valid()
                        && !session.members.contains_key(device.as_str()) =>
                    {
                        added.push(EpochMember {
                            id: device.as_str().to_owned(),
                            public,
                            seat: Some(seat.clone()),
                        });
                    }
                    Ok(SeatRequest::Remove { device })
                        if session
                            .seats
                            .get(device.as_str())
                            .is_some_and(|s| s.person == seat.person) =>
                    {
                        removed.push(device.as_str().to_owned());
                    }
                    _ => {}
                }
            }
        } else {
            for (number, request) in &accepted {
                let ask = SeatRequest::Add {
                    device: request.device.clone(),
                    public: request.public.clone(),
                    person: me.seat.person.clone(),
                };
                let path = team_layout::seat_request(&me.device, &number.to_string());
                self.write_signed(&session, &path, &json(&ask))?;
            }
        }
        if accepted.is_empty() && added.is_empty() && removed.is_empty() && done.is_empty() {
            return Ok(());
        }
        let mut pins = self.pins(space)?;
        if !added.is_empty() || !removed.is_empty() {
            let (number, file) = self.next_team_epoch(space, &me, &added, &removed)?;
            pins.insert(number, self.sync_crypto.digest(file.as_bytes()));
        }
        let keys = self.keys(space)?;
        let answer = JoinAnswer {
            recovery: self.sync_setting(space, RECOVERY)?.unwrap_or_default(),
            pins,
        };
        for (number, _) in &accepted {
            let Some(key) = keys.invites.get(number).and_then(|i| i.key.clone()) else {
                continue;
            };
            let path = team_layout::answer(*number);
            let sealed = self
                .sync_crypto
                .invite_seal(&key, &path, json(&answer).as_bytes())?;
            self.sync_remote.write(&dir, &path, &sealed)?;
        }
        for path in &done {
            self.sync_remote.remove(&dir, path)?;
        }
        if let Err(error) = self.sync_remote.push(&dir, "hedy team") {
            let _ = self.sync_remote.discard_local(&dir);
            return Err(error.into());
        }
        for (number, _) in &accepted {
            self.forget_invite(space, *number)?;
        }
        Ok(())
    }

    /// Writes the epoch after the trusted one, signed by this admin's PC,
    /// with `added` in and `removed` out. Returns its number and file.
    fn next_team_epoch(
        &self,
        space: &Space,
        me: &Me,
        added: &[EpochMember],
        removed: &[String],
    ) -> AppResult<(u32, String)> {
        let dir = self.sync_dir(space)?;
        let keys = self.keys(space)?;
        let newest = self.sync_number(space, EPOCH)? as u32;
        let epochs = self.epochs(&dir)?;
        let file = epochs
            .iter()
            .find(|(n, _)| *n == newest)
            .map(|(_, f)| f.clone())
            .ok_or_else(|| corrupt(&layout::epoch(newest)))?;
        // Another PC changed the keys meanwhile: the next sync checks it.
        if let Some((_, next)) = epochs.iter().find(|(n, _)| *n == newest + 1)
            && self.sync_crypto.verify_next_epoch(&file, next).is_ok()
        {
            return Err(AppError::SyncBusy);
        }
        let EpochInfo { team, members, .. } = self.sync_crypto.epoch_members(&file)?;
        let mut members: Vec<EpochMember> = members
            .into_iter()
            .filter(|m| !removed.contains(&m.id))
            .collect();
        for member in added {
            if !members.iter().any(|m| m.id == member.id) {
                members.push(member.clone());
            }
        }
        let (next, next_key) = self.sync_crypto.new_epoch(
            newest + 1,
            team,
            &members,
            me.device.as_str(),
            &keys.device,
        )?;
        self.sync_remote
            .write(&dir, &layout::epoch(newest + 1), next.as_bytes())?;
        // Who opens the new epoch can read what was sealed before it (a
        // new member); who is left out of it reads nothing new.
        let previous = keys
            .epochs
            .get(&newest)
            .cloned()
            .ok_or(AppError::SyncRemoved)?;
        let path = team_layout::key_history(newest + 1);
        let body = json(&KeyHistory {
            epoch: newest,
            key: previous,
        });
        let envelope = json(&Signed {
            signature: self
                .sync_crypto
                .sign(&keys.device, &signed_bytes(&path, &body))?,
            body,
        });
        let sealed = self
            .sync_crypto
            .seal(&next_key, &path, envelope.as_bytes())?;
        self.sync_remote.write(&dir, &path, &sealed)?;
        Ok((newest + 1, next))
    }

    /// The keys of the epochs before the first this PC is in, from their
    /// history files: each one opened with the key after it and signed by
    /// an admin's PC of that epoch. Returns whether it found any.
    pub(super) fn fill_older_keys(
        &self,
        dir: &Path,
        epochs: &[(u32, String)],
        newest: u32,
        keys: &mut Keys,
    ) -> AppResult<bool> {
        let mut found = false;
        let mut epoch = newest;
        while epoch > 1 {
            let before = epoch - 1;
            if keys.epochs.contains_key(&before) {
                epoch = before;
                continue;
            }
            let key = keys.epochs.get(&epoch).cloned();
            let files: Vec<&String> = epochs
                .iter()
                .filter(|(n, _)| *n == epoch || *n == before)
                .map(|(_, f)| f)
                .collect();
            let Some(key) = key else { break };
            let path = team_layout::key_history(epoch);
            let opened = self.read_repo(dir, &path)?.and_then(|sealed| {
                let plain = self.sync_crypto.open(&[key], &path, &sealed).ok()?;
                let signed: Signed = serde_json::from_slice(&plain).ok()?;
                // An admin's PC of either epoch (one that left with it, too).
                let members: Vec<EpochMember> = files
                    .iter()
                    .filter_map(|f| self.sync_crypto.epoch_members(f).ok())
                    .flat_map(|info| info.members)
                    .collect();
                let signer = members.iter().any(|m| {
                    m.seat.as_ref().is_some_and(|s| s.role == Role::Admin)
                        && self
                            .sync_crypto
                            .verify(
                                &m.public,
                                &signed_bytes(&path, &signed.body),
                                &signed.signature,
                            )
                            .is_ok()
                });
                let history: KeyHistory = serde_json::from_str(&signed.body).ok()?;
                (signer && history.epoch == before).then_some(history.key)
            });
            let Some(older) = opened else { break };
            keys.epochs.insert(before, older);
            found = true;
            epoch = before;
        }
        Ok(found)
    }

    /// Joins the team at `url` with an invite's code. The request leaves
    /// now; the PC is in once the inviting PC (or, for another PC of a
    /// member, an admin's) syncs: [`Self::team_sync`] then finishes it.
    pub fn team_join(
        &self,
        url: &str,
        code: &str,
        person_name: &str,
        device_name: &str,
    ) -> AppResult<TeamJoin> {
        let (number, code) = self
            .sync_crypto
            .parse_invite_code(code)
            .ok_or(AppError::TeamInviteCode)?;
        if url_has_credentials(url) {
            return Err(AppError::SyncUrlWithCredentials);
        }
        let id = self.sync_crypto.random_hex(8)?;
        let space = Space::Team(id.clone());
        let _guard = self.sync_guard(&space)?;
        let result = self.join_team_locked(&space, url, number, &code, person_name, device_name);
        if result.is_err() {
            let _ = self.sync_remote.discard_local(&self.sync_dir(&space)?);
            let _ = self.secrets.delete_sync_keys(&space.id());
            for key in [JOIN, DEVICE, NAME, PERSON_NAME] {
                let _ = self.clear_sync_setting(&space, key);
            }
            return result.map(|_| unreachable!());
        }
        let mut ids = self.team_ids()?;
        ids.push(id.clone());
        self.save_team_ids(&ids)?;
        // The inviting PC may have answered already (it synced meanwhile).
        let joined = self.finish_join(&space).unwrap_or(false);
        Ok(TeamJoin { id, joined })
    }

    fn join_team_locked(
        &self,
        space: &Space,
        url: &str,
        number: u32,
        code: &str,
        person_name: &str,
        device_name: &str,
    ) -> AppResult<()> {
        let dir = self.sync_dir(space)?;
        self.sync_remote.open(url, &dir)?;
        let epochs = self.epochs(&dir)?;
        match epochs.first() {
            Some((1, first)) if self.sync_crypto.epoch_members(first)?.team => {}
            Some(_) => return Err(AppError::TeamNotATeam),
            None => return Err(AppError::SyncRepoEmpty),
        }
        let context = self.invite_context(&dir)?;
        let offer: Offer = self
            .read_repo(&dir, &team_layout::offer(number))?
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or(AppError::TeamInviteNotFound(number))?;
        if offer.expires < self.now_secs() {
            return Err(AppError::TeamInviteExpired);
        }
        if self
            .read_repo(&dir, &team_layout::request(number))?
            .is_some()
        {
            return Err(AppError::TeamInviteUsed);
        }
        let (message, key) = self
            .sync_crypto
            .invite_join(code, &context, &offer.message)?;
        let device = DeviceId(self.sync_crypto.random_hex(8)?);
        let me = self.sync_crypto.new_member()?;
        let request = JoinRequest {
            device: device.clone(),
            public: self.sync_crypto.member_public(&me)?,
            device_name: device_name.trim().to_owned(),
            person_name: person_name.trim().to_owned(),
        };
        let path = team_layout::request(number);
        let sealed = self
            .sync_crypto
            .invite_seal(&key, &path, json(&request).as_bytes())?;
        let file = RequestFile {
            message,
            sealed: BASE64.encode(sealed),
        };
        self.sync_remote
            .write(&dir, &path, json(&file).as_bytes())?;
        let keys = Keys {
            device: me,
            invite: Some(key),
            ..Keys::default()
        };
        self.save_keys(space, &keys)?;
        self.set_sync_setting(space, DEVICE, device.as_str())?;
        self.set_sync_setting(space, NAME, device_name.trim())?;
        self.set_sync_setting(space, PERSON_NAME, person_name.trim())?;
        self.set_sync_setting(
            space,
            JOIN,
            &json(&Joining {
                url: url.to_owned(),
                number,
            }),
        )?;
        // Another joiner wrote first: this one is out.
        self.sync_remote
            .push(&dir, "hedy team")
            .map_err(|_| AppError::TeamInviteUsed)
    }

    /// Whether a PC joining is in now: the answer gives it the team's
    /// recovery key and the epochs' pins, from which it checks the chain
    /// itself; it is in once an epoch lists it.
    fn finish_join(&self, space: &Space) -> AppResult<bool> {
        let Some(joining) = self.sync_json::<Joining>(space, JOIN)? else {
            return self.space_is_set_up(space);
        };
        let mut keys = self.keys(space)?;
        let key = keys.invite.clone().ok_or(AppError::SyncNotSetUp)?;
        let device = DeviceId(
            self.sync_setting(space, DEVICE)?
                .ok_or(AppError::SyncNotSetUp)?,
        );
        let dir = self.sync_dir(space)?;
        self.sync_remote.open(&joining.url, &dir)?;
        let path = team_layout::answer(joining.number);
        let Some(sealed) = self.read_repo(&dir, &path)? else {
            // Burned (or cancelled): its files are gone without an answer.
            if self
                .read_repo(&dir, &team_layout::request(joining.number))?
                .is_none()
            {
                return Err(AppError::TeamInviteFailed);
            }
            return Ok(false);
        };
        let answer: JoinAnswer = self
            .sync_crypto
            .invite_open(&key, &path, &sealed)
            .ok()
            .and_then(|plain| serde_json::from_slice(&plain).ok())
            .ok_or(AppError::TeamInviteFailed)?;
        let epochs = self.epochs(&dir)?;
        let first = epochs
            .first()
            .filter(|(n, _)| *n == 1)
            .map(|(_, f)| f)
            .ok_or_else(|| corrupt(&layout::epoch(1)))?;
        self.sync_crypto
            .verify_first_epoch(first, &answer.recovery)?;
        for (number, pin) in &answer.pins {
            let file = epochs
                .iter()
                .find(|(n, _)| n == number)
                .map(|(_, f)| f)
                .ok_or_else(|| corrupt(&layout::epoch(*number)))?;
            if self.sync_crypto.digest(file.as_bytes()) != *pin {
                return Err(AppError::SyncTampered(*number));
            }
        }
        let newest = answer.pins.keys().max().copied().unwrap_or(1);
        // Every pinned epoch it is in, so it reads what was sealed then.
        for (number, file) in epochs.iter().filter(|(n, _)| answer.pins.contains_key(n)) {
            if let Ok(epoch_key) = self
                .sync_crypto
                .open_epoch(file, device.as_str(), &keys.device)
            {
                keys.epochs.insert(*number, epoch_key);
            }
        }
        self.save_keys(space, &keys)?;
        self.set_sync_setting(space, RECOVERY, &answer.recovery)?;
        self.set_sync_setting(space, PINS, &json(&answer.pins))?;
        self.set_sync_setting(space, EPOCH, &newest.to_string())?;
        match self.sync_session(space, dir.clone(), device.clone()) {
            Ok(session) => {
                let Some(seat) = session.seats.get(device.as_str()) else {
                    return Ok(false);
                };
                self.set_sync_setting(space, PERSON, seat.person.as_str())?;
            }
            // Not in an epoch yet: a member's PC asked an admin's to add it.
            Err(AppError::SyncRemoved) => return Ok(false),
            Err(error) => return Err(error),
        }
        let mut keys = self.keys(space)?;
        keys.invite = None;
        self.save_keys(space, &keys)?;
        self.sync_remote.remove(&dir, &path)?;
        self.set_sync_setting(space, SEQ, "0")?;
        self.clear_sync_setting(space, JOIN)?;
        // Last: what says the team is set up here.
        self.set_sync_setting(space, URL, &joining.url)?;
        self.sync_locked(space)?;
        Ok(true)
    }

    /// This PC leaves the team here: its keys, clone, settings and replica
    /// go, and its projects' memory follows their owner again.
    pub fn team_leave(&self, team: &str) -> AppResult<()> {
        let space = self.team_space_or_joining(team)?;
        let Space::Team(id) = &space else {
            unreachable!("a team's space");
        };
        let _guard = self.sync_guard(&space)?;
        self.sync_remote.discard_local(&self.sync_dir(&space)?)?;
        self.secrets.delete_sync_keys(&space.id())?;
        self.store.clear_sync_state(&space.id())?;
        for key in [
            URL,
            DEVICE,
            NAME,
            RECOVERY,
            EPOCH,
            PINS,
            SEQ,
            PENDING,
            OUTBOX,
            CLOCK,
            LAST_SYNC,
            LAST_ERROR,
            INFO_AT,
            SNAPSHOT,
            PERSON,
            PERSON_NAME,
            TEAM_NAME,
            NEW_NAME,
            INVITES,
            FAILED,
            JOIN,
        ] {
            self.clear_sync_setting(&space, key)?;
        }
        let team_home = MemoryHome::Team(id.clone());
        for project in self.store.list_projects()? {
            if self.store.memory_home(&project.path)? == Some(team_home.clone()) {
                self.store.set_memory_home(&project.path, None)?;
            }
        }
        let ids: Vec<String> = self.team_ids()?.into_iter().filter(|i| i != id).collect();
        self.save_team_ids(&ids)
    }
}

/// A note or suggestion text with hidden characters is refused, showing
/// where they are.
fn check_clean(text: &str) -> AppResult<()> {
    if hidden_chars(text).is_empty() {
        Ok(())
    } else {
        Err(AppError::TeamHiddenText(reveal_hidden(text)))
    }
}

/// The project here a scope names, when there is exactly one.
fn project_here(projects: &HashMap<String, Vec<PathBuf>>, scope: &Scope) -> Option<PathBuf> {
    match scope {
        Scope::Global => None,
        Scope::Project(id) => match projects.get(id).map(Vec::as_slice) {
            Some([only]) => Some(only.clone()),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests;
