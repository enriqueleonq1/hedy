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

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

use super::*;
use crate::domain::memory::{Category, normalize_topic};
use crate::domain::team::{
    INVITE_TTL_SECS, PersonId, Role, Seat, SuggestionBody, TeamNoteBody, TeamWrite, hidden_chars,
    item_uid, layout as team_layout, reveal_hidden, uid_person,
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
/// A member's PC's asks for its person's new PCs (JSON [`SeatAsk`]s).
const ASKS: &str = "asks";
/// The people's names as admins wrote them (JSON person → name).
const ROSTER: &str = "roster";
/// Names an admin's PC gave, to write in the team's file (JSON person →
/// name).
const ROSTER_DUE: &str = "roster_due";
/// An admin's PC asked to leave and waits to be taken out.
const LEAVING: &str = "leaving";

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
        /// The epoch it was asked in: only then is it signed.
        epoch: u32,
    },
    Remove {
        device: DeviceId,
    },
}

/// A member's PC asking an admin's to add one of its person's PCs, until
/// it is in: the request in the repo is for epoch `epoch`.
#[derive(Serialize, Deserialize)]
struct SeatAsk {
    device: DeviceId,
    public: String,
    epoch: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
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

/// The team's name and its people's names, as an admin's PC writes them in
/// its folder (sealed and signed): the newest name any admin wrote is the
/// team's, and the people's names add up. A member's name is the one the
/// admin who let them in wrote, never one they give themselves.
#[derive(Serialize, Deserialize)]
struct TeamInfo {
    name: String,
    /// When it was written (ms).
    at: i64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    people: BTreeMap<PersonId, String>,
}

/// A team this PC is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamSummary {
    pub id: String,
    pub name: String,
    pub url: String,
    /// Joining: waiting for the inviting PC to let this one in.
    pub waiting: bool,
    /// Leaving (an admin's PC): waiting for another admin's to take it out.
    pub leaving: bool,
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

    /// Adds (`true`) or removes a team id from the list, holding a lock
    /// of its own: the app and `hedy` may change the list at once, each
    /// for another team.
    fn edit_team_ids(&self, id: &str, add: bool) -> AppResult<()> {
        let data = self
            .store
            .data_dir()
            .and_then(|dir| std::path::absolute(dir).ok())
            .ok_or_else(|| corrupt("data dir"))?;
        let _lock = self.sync_remote.lock(&data.join("teams").join("list"))?;
        let mut ids: Vec<String> = self.team_ids()?.into_iter().filter(|i| i != id).collect();
        if add {
            ids.push(id.to_owned());
        }
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
        self.edit_team_ids(id, true)?;
        self.set_sync_setting(space, PERSON, person.as_str())?;
        self.set_sync_setting(space, PERSON_NAME, person_name.trim())?;
        self.set_sync_setting(space, TEAM_NAME, team_name.trim())?;
        self.set_sync_setting(space, NEW_NAME, team_name.trim())?;
        let roster = BTreeMap::from([(person.clone(), shown_name(person_name))]);
        self.set_sync_setting(space, ROSTER, &json(&roster))?;
        self.set_sync_setting(space, ROSTER_DUE, &json(&roster))?;
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
                    leaving: false,
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
            leaving: self.sync_setting(space, LEAVING)?.is_some(),
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
        let roster = self.roster(&space)?;
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
        let mut people: Vec<TeamPerson> = people
            .into_values()
            .map(|mut p| {
                if let Some(name) = roster.get(&p.person) {
                    p.name = name.clone();
                }
                p
            })
            .collect();
        people.sort_by(|a, b| {
            (a.role != Role::Admin, a.name.to_lowercase())
                .cmp(&(b.role != Role::Admin, b.name.to_lowercase()))
        });
        // A PC removed from the team still shows what it last knew.
        let suggestions = self.team_suggestions(team).map_or(0, |s| s.len());
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
        let leaving = self.sync_setting(&space, LEAVING)?.is_some();
        let result = if self.space_is_set_up(&space)? {
            self.sync_locked(&space)
                .and_then(|report| self.team_seats(&space).map(|()| report))
        } else {
            self.finish_join(&space).map(|_| SyncReport::default())
        };
        if leaving {
            return self.finish_leaving(&space, result);
        }
        match &result {
            Ok(_) => self.clear_sync_setting(&space, LAST_ERROR)?,
            Err(error) => self.set_sync_setting(&space, LAST_ERROR, &shown_error(error))?,
        }
        result
    }

    /// An admin's PC that asked to leave: out now (it forgets the team), or
    /// its request was dropped (leaving would have left no admin), or it
    /// still waits.
    fn finish_leaving(
        &self,
        space: &Space,
        result: AppResult<SyncReport>,
    ) -> AppResult<SyncReport> {
        match result {
            Err(AppError::SyncRemoved) => {
                self.forget_team(space)?;
                Ok(SyncReport::default())
            }
            Ok(report) => {
                let me = self.team_me(space)?;
                let dir = self.sync_dir(space)?;
                let mine = format!("{}/{}/", team_layout::REQUESTS_DIR, me.device.as_str());
                let asked = self.sync_remote.files(&dir, &mine)?;
                // No other admin's PC is left to take it out: it stays.
                let (_, members) = self.space_status(space)?.ok_or(AppError::SyncNotSetUp)?;
                let alone = !members.iter().any(|m| {
                    !m.device.this_pc && m.seat.as_ref().is_some_and(|s| s.role == Role::Admin)
                });
                if alone && !asked.is_empty() {
                    for path in &asked {
                        self.sync_remote.remove(&dir, path)?;
                    }
                    self.sync_remote.push(&dir, "hedy team")?;
                }
                if asked.is_empty() || alone {
                    self.clear_sync_setting(space, LEAVING)?;
                    let error = AppError::TeamLastAdmin;
                    self.set_sync_setting(space, LAST_ERROR, &shown_error(&error))?;
                    return Err(error);
                }
                Ok(report)
            }
            Err(error) => Err(error),
        }
    }

    /// Whether a version another PC of the team wrote may be taken, by
    /// that PC's seat in the newest epoch (docs/decisions.md §35): checked
    /// by every PC that reads it.
    pub(super) fn team_allows(
        &self,
        session: &Session,
        change: &Change,
        known: Option<(&Scope, &Version)>,
    ) -> AppResult<bool> {
        let Some(writer) = session.seats.get(change.version.stamp.device.as_str()) else {
            return Ok(false);
        };
        let (known_scope, known) = (known.map(|k| k.0), known.map(|k| k.1));
        // A member never moves an item to another layer or project (out of
        // the team layer, where admins review it).
        if writer.role != Role::Admin && known_scope.is_some_and(|s| *s != change.scope) {
            return Ok(false);
        }
        let team_layer = change.scope == Scope::Global;
        let uid_owner = uid_person(&change.uid);
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
                    uid_owner: uid_owner.as_ref(),
                }
                .note_allowed()
            }
            ItemKind::Suggestion => {
                let new = match &change.version.body {
                    None => None,
                    // Its text may hide characters (a reviewer sees them,
                    // and it cannot be taken); its byline may not.
                    Some(_) => match suggestion_body(&change.version) {
                        Some(s)
                            if s.person.is_valid()
                                && note_is_clean("", &s.author, s.topic.as_deref()) =>
                        {
                            Some(s)
                        }
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
                }
                .or_else(|| {
                    new.as_ref()
                        .or(old.as_ref())
                        .and_then(|s| s.note_owner.clone())
                });
                TeamWrite {
                    writer,
                    team_layer,
                    owner: new.as_ref().map(|s| &s.person),
                    replaced_owner: old.as_ref().map(|s| &s.person),
                    note_owner: note_owner.as_ref(),
                    uid_owner: uid_owner.as_ref(),
                }
                .suggestion_allowed()
            }
            // A team holds notes and suggestions only.
            _ => false,
        })
    }

    /// What a team sync does after applying the other PCs' versions: an
    /// admin's PC keeps the team's name and its people's names in the repo;
    /// every PC keeps what it reads there.
    pub(super) fn team_duties(&self, session: &Session) -> AppResult<()> {
        let space = &session.space;
        let admin = session
            .seats
            .get(session.device.as_str())
            .is_some_and(|s| s.role == Role::Admin);
        let infos = self.read_team_infos(session);
        let mut roster: BTreeMap<PersonId, String> = BTreeMap::new();
        for info in &infos {
            roster.extend(info.people.clone());
        }
        let mine: BTreeMap<PersonId, String> = self.sync_json(space, ROSTER)?.unwrap_or_default();
        // A name only this PC still holds (the admin who wrote it left):
        // an admin's PC writes it again.
        let missing = mine.keys().any(|p| !roster.contains_key(p));
        for (person, name) in mine {
            roster.entry(person).or_insert(name);
        }
        self.set_sync_setting(space, ROSTER, &json(&roster))?;
        let newest = infos.last();
        // A new name; or, when no admin's name is left in the repo (the PC
        // that wrote it was removed), the one this PC knows.
        let name = match (self.sync_setting(space, NEW_NAME)?, newest) {
            (Some(name), _) => Some(name),
            (None, None) => self
                .sync_setting(space, TEAM_NAME)?
                .filter(|n| !n.is_empty()),
            (None, Some(_)) => None,
        };
        // Names this admin's PC gave (a new person, a rename) win.
        let given: BTreeMap<PersonId, String> =
            self.sync_json(space, ROSTER_DUE)?.unwrap_or_default();
        let names_due = missing || !given.is_empty();
        roster.extend(given);
        self.set_sync_setting(space, ROSTER, &json(&roster))?;
        if admin && (name.is_some() || names_due) {
            let name = name
                .or_else(|| newest.map(|i| i.name.clone()))
                .unwrap_or_default();
            let info = TeamInfo {
                name: name.clone(),
                at: self.clock.now_ms(),
                people: roster,
            };
            self.write_signed(
                session,
                &team_layout::team_info(&session.device),
                &json(&info),
            )?;
            self.set_sync_setting(space, TEAM_NAME, &name)?;
            self.clear_sync_setting(space, NEW_NAME)?;
            self.clear_sync_setting(space, ROSTER_DUE)?;
        } else if let Some(info) = newest {
            self.set_sync_setting(space, TEAM_NAME, &info.name)?;
        }
        self.open_requests(session)
    }

    /// What the admins' PCs of the newest epoch signed about the team,
    /// oldest first.
    fn read_team_infos(&self, session: &Session) -> Vec<TeamInfo> {
        let mut infos: Vec<TeamInfo> = session
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
            .collect();
        infos.sort_by(|a, b| (a.at, &a.name).cmp(&(b.at, &b.name)));
        infos
    }

    /// The people's names as admins wrote them.
    fn roster(&self, space: &Space) -> AppResult<BTreeMap<PersonId, String>> {
        Ok(self.sync_json(space, ROSTER)?.unwrap_or_default())
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
        let uid = item_uid(&me.seat.person, &self.sync_crypto.random_hex(8)?);
        if scope == Scope::Global && me.seat.role != Role::Admin {
            let suggestion = SuggestionBody {
                note: None,
                note_owner: None,
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
    fn may_edit(me: &Me, scope: &Scope, uid: &str, note: &TeamNoteBody) -> bool {
        TeamWrite {
            writer: &me.seat,
            team_layer: *scope == Scope::Global,
            owner: Some(&note.person),
            replaced_owner: Some(&note.person),
            note_owner: None,
            uid_owner: uid_person(uid).as_ref(),
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
        if !Self::may_edit(&me, &scope, uid, &note) {
            let suggestion = SuggestionBody {
                note: Some(uid.to_owned()),
                note_owner: Some(note.person.clone()),
                text: text.map(str::to_owned),
                category,
                topic,
                person: me.seat.person.clone(),
                author: me.name.clone(),
                created_at: self.now_secs(),
            };
            let sid = item_uid(&me.seat.person, &self.sync_crypto.random_hex(8)?);
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
        let roster = self.roster(&space)?;
        let projects = self.team_projects(&space)?;
        let mut notes: Vec<TeamNote> = self
            .store
            .sync_items(&space.id())?
            .into_iter()
            .filter(|c| c.kind == ItemKind::Note)
            .filter_map(|c| {
                let note = note_body(&c.version)?;
                let can_edit = Self::may_edit(&me, &c.scope, &c.uid, &note);
                Some(TeamNote {
                    uid: c.uid,
                    team_layer: c.scope == Scope::Global,
                    project: project_here(&projects, &c.scope),
                    mine: note.person == me.seat.person,
                    can_edit,
                    text: note.text,
                    category: note.category,
                    topic: note.topic,
                    person: note.person.clone(),
                    // The name an admin wrote for that person, if any.
                    author: roster
                        .get(&note.person)
                        .cloned()
                        .unwrap_or(note.author.clone()),
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
        let roster = self.roster(&space)?;
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
                person: s.person.clone(),
                author: roster.get(&s.person).cloned().unwrap_or(s.author),
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
        check_clean(&s.author)?;
        if let Some(topic) = &s.topic {
            check_clean(topic)?;
        }
        // The suggestion goes first: a PC that reads both meets it while its
        // note is still there (whose owner may remove it).
        self.team_write(&me, ItemKind::Suggestion, uid, scope.clone(), None)?;
        let now = self.now_secs();
        match (&s.note, note, s.text) {
            // A new note, in its author's name.
            (None, _, Some(text)) => {
                let note_uid = item_uid(&s.person, &self.sync_crypto.random_hex(8)?);
                let body = TeamNoteBody {
                    text,
                    category: s.category,
                    person: s.person,
                    author: s.author,
                    created_at: now,
                    topic: s.topic,
                    updated_at: None,
                };
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
        Ok(())
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
    /// before it is checked, and one that does not open stays burned (its
    /// files go). One that opens keeps its files until its answer is out,
    /// so the joiner never mistakes the wait for a failure. An invite whose
    /// offer is gone, or that expired with no request, goes.
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
            let request = self.read_repo(&session.dir, &team_layout::request(number))?;
            if offer.is_none() || (record.expires < now && request.is_none()) {
                self.drop_invite_files(session, number)?;
                self.forget_invite(space, number)?;
                records.remove(&number);
                continue;
            }
            let Some(request) = request else {
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
                    && self
                        .sync_crypto
                        .check_member_public(&request.public)
                        .is_ok())
                .then_some((key, request))
            });
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
                    self.drop_invite_files(session, number)?;
                    self.forget_invite(space, number)?;
                    records.remove(&number);
                    self.note_failed_invite(space, number)?;
                }
            }
            self.save_invite_records(space, &records)?;
        }
        self.save_invite_records(space, &records)
    }

    fn note_failed_invite(&self, space: &Space, number: u32) -> AppResult<()> {
        let mut failed: Vec<u32> = self.sync_json(space, FAILED)?.unwrap_or_default();
        failed.push(number);
        self.set_sync_setting(space, FAILED, &json(&failed))
    }

    fn drop_invite_files(&self, session: &Session, number: u32) -> AppResult<()> {
        for path in [team_layout::offer(number), team_layout::request(number)] {
            if self.read_repo(&session.dir, &path)?.is_some() {
                self.sync_remote.remove(&session.dir, &path)?;
            }
        }
        Ok(())
    }

    /// Every PC id in the epochs up to `newest`: a PC once removed is never
    /// added again (an old request of its own, replayed, must not bring it
    /// back).
    fn devices_ever(&self, dir: &Path, newest: u32) -> AppResult<HashSet<String>> {
        let mut ids = HashSet::new();
        for (number, file) in self.epochs(dir)? {
            if number <= newest {
                ids.extend(
                    self.sync_crypto
                        .epoch_members(&file)?
                        .members
                        .into_iter()
                        .map(|m| m.id),
                );
            }
        }
        Ok(ids)
    }

    /// After a sync: the seat changes this PC makes, in a commit of their
    /// own (another admin's PC may write the same epoch first; then they
    /// are made again on the next sync).
    /// - An admin's PC adds the PCs whose invites it answered, and signs
    ///   what members' PCs asked about their own PCs (an addition asked for
    ///   the epoch in use only, so an old request replayed does nothing).
    /// - A member's PC asks an admin's to add the PC it invited, again for
    ///   each new epoch, until it is in.
    ///
    /// Then each answered invite gets its answer, and its files go.
    fn team_seats(&self, space: &Space) -> AppResult<()> {
        let records = self.invite_records(space)?;
        let me = self.team_me(space)?;
        let dir = self.sync_dir(space)?;
        let session = self.sync_session(space, dir.clone(), me.device.clone())?;
        let newest = self.sync_number(space, EPOCH)? as u32;
        let admin = me.seat.role == Role::Admin;
        let ever = self.devices_ever(&dir, newest)?;
        let mut added: Vec<EpochMember> = Vec::new();
        let mut names: Vec<(PersonId, String)> = Vec::new();
        let mut removed: Vec<String> = Vec::new();
        let mut done: Vec<String> = Vec::new();
        let mut answered: Vec<u32> = Vec::new();
        let mut failed: Vec<u32> = Vec::new();
        // A new PC with keys of its own (no member's, no other new PC's).
        let fits = |device: &DeviceId, public: &str, added: &[EpochMember]| {
            device.is_valid()
                && !ever.contains(device.as_str())
                && !added
                    .iter()
                    .any(|m| m.id == device.as_str() || m.public == public)
                && !session.members.values().any(|p| p == public)
                && self.sync_crypto.check_member_public(public).is_ok()
        };
        let mut asks: Vec<SeatAsk> = self.sync_json(space, ASKS)?.unwrap_or_default();
        for (number, record) in &records {
            let Some(request) = &record.accepted else {
                continue;
            };
            if session.members.contains_key(request.device.as_str()) {
                // In already (an answer whose push was cut short).
                answered.push(*number);
            } else if !fits(&request.device, &request.public, &added) {
                failed.push(*number);
            } else if admin {
                let person = if record.own_pc {
                    me.seat.person.clone()
                } else {
                    let person = PersonId(self.sync_crypto.random_hex(8)?);
                    let name = self.new_name(space, &names, &request.person_name)?;
                    names.push((person.clone(), name));
                    person
                };
                let role = if record.own_pc {
                    me.seat.role
                } else {
                    record.role
                };
                added.push(EpochMember {
                    id: request.device.as_str().to_owned(),
                    public: request.public.clone(),
                    seat: Some(Seat { person, role }),
                });
                answered.push(*number);
            } else {
                if !asks.iter().any(|a| a.device == request.device) {
                    asks.push(SeatAsk {
                        device: request.device.clone(),
                        public: request.public.clone(),
                        epoch: 0,
                        path: None,
                    });
                }
                answered.push(*number);
            }
        }
        if admin {
            // What it asked for while a member (promoted since): its own.
            for ask in &asks {
                if !session.members.contains_key(ask.device.as_str())
                    && fits(&ask.device, &ask.public, &added)
                {
                    added.push(EpochMember {
                        id: ask.device.as_str().to_owned(),
                        public: ask.public.clone(),
                        seat: Some(me.seat.clone()),
                    });
                }
            }
            for path in self.sync_remote.files(&dir, team_layout::REQUESTS_DIR)? {
                // This PC's own requests are another admin's PC's to sign
                // (and to remove).
                if team_layout::seat_request_of(&path)
                    .is_some_and(|(signer, _)| signer == me.device)
                {
                    continue;
                }
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
                        epoch,
                    }) if person == seat.person
                        && epoch == newest
                        && fits(&device, &public, &added) =>
                    {
                        added.push(EpochMember {
                            id: device.as_str().to_owned(),
                            public,
                            seat: Some(seat.clone()),
                        });
                    }
                    // One that would leave no admin (two admins leaving at
                    // once) is dropped on its own; its PC learns it stayed.
                    // Never its own: the PC that takes another out answers
                    // for what it wrote, which a PC that is out cannot.
                    Ok(SeatRequest::Remove { device })
                        if device != me.device
                            && session
                                .seats
                                .get(device.as_str())
                                .is_some_and(|s| s.person == seat.person)
                            && !removed.contains(&device.0)
                            && self
                                .check_admins_stay(
                                    space,
                                    &[removed.clone(), vec![device.0.clone()]].concat(),
                                    &[],
                                )
                                .is_ok() =>
                    {
                        removed.push(device.0);
                    }
                    _ => {}
                }
            }
        }
        // A member's asks: done once the PC is in; asked again for each
        // new epoch (under a new name, so no file is ever written twice).
        asks.retain(|a| !session.members.contains_key(a.device.as_str()));
        let mut stale: Vec<String> = Vec::new();
        let mut write_asks: Vec<(String, SeatRequest)> = Vec::new();
        if !admin {
            for ask in &mut asks {
                if ask.epoch == newest && ask.path.is_some() {
                    continue;
                }
                stale.extend(ask.path.take());
                let path = team_layout::seat_request(&me.device, &self.sync_crypto.random_hex(8)?);
                write_asks.push((
                    path.clone(),
                    SeatRequest::Add {
                        device: ask.device.clone(),
                        public: ask.public.clone(),
                        person: me.seat.person.clone(),
                        epoch: newest,
                    },
                ));
                ask.epoch = newest;
                ask.path = Some(path);
            }
        }
        let new_epoch = !added.is_empty() || !removed.is_empty();
        if !new_epoch
            && answered.is_empty()
            && failed.is_empty()
            && done.is_empty()
            && write_asks.is_empty()
        {
            return Ok(());
        }
        let wrote = (|| -> AppResult<()> {
            let mut pins = self.pins(space)?;
            if new_epoch {
                self.check_admins_stay(space, &removed, &[])?;
                let (number, file) = self.next_team_epoch(space, &me, &added, &removed, &[])?;
                pins.insert(number, self.sync_crypto.digest(file.as_bytes()));
                self.drop_device_files(&dir, &removed)?;
                // Restating is harmless if the push fails: same content.
                self.take_over(&me, &removed, false)?;
            }
            let keys = self.keys(space)?;
            let answer = JoinAnswer {
                recovery: self.sync_setting(space, RECOVERY)?.unwrap_or_default(),
                pins,
            };
            for number in &answered {
                let request = records[number].accepted.as_ref().expect("answered");
                let key = keys.invites.get(number).and_then(|i| i.key.clone());
                // A PC already in has what it needs (and may have removed it).
                if let Some(key) = key
                    && !session.members.contains_key(request.device.as_str())
                {
                    let path = team_layout::answer(*number);
                    let sealed =
                        self.sync_crypto
                            .invite_seal(&key, &path, json(&answer).as_bytes())?;
                    self.sync_remote.write(&dir, &path, &sealed)?;
                }
            }
            for number in answered.iter().chain(&failed) {
                self.drop_invite_files(&session, *number)?;
            }
            for path in done.iter().chain(&stale) {
                if self.read_repo(&dir, path)?.is_some() {
                    self.sync_remote.remove(&dir, path)?;
                }
            }
            for (path, ask) in &write_asks {
                self.write_signed(&session, path, &json(ask))?;
            }
            Ok(self.sync_remote.push(&dir, "hedy team")?)
        })();
        if let Err(error) = wrote {
            let _ = self.sync_remote.discard_local(&dir);
            return Err(error);
        }
        for number in answered.iter().chain(&failed) {
            self.forget_invite(space, *number)?;
        }
        for number in &failed {
            self.note_failed_invite(space, *number)?;
        }
        self.set_sync_setting(space, ASKS, &json(&asks))?;
        if !names.is_empty() {
            self.name_people(space, names)?;
        }
        // Its own epoch, checked and in use here; and what it took over, sent.
        if new_epoch {
            self.sync_locked(space)?;
        }
        Ok(())
    }

    /// Writes the epoch after the trusted one, signed by this admin's PC,
    /// with `added` in, `removed` out and the `roles` given. Returns its
    /// number and file.
    fn next_team_epoch(
        &self,
        space: &Space,
        me: &Me,
        added: &[EpochMember],
        removed: &[String],
        roles: &[(PersonId, Role)],
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
        for member in &mut members {
            if let Some(seat) = &mut member.seat
                && let Some((_, role)) = roles.iter().find(|(p, _)| *p == seat.person)
            {
                seat.role = *role;
            }
        }
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
            let info = |n: u32| {
                epochs
                    .iter()
                    .find(|(m, _)| *m == n)
                    .and_then(|(_, f)| self.sync_crypto.epoch_members(f).ok())
            };
            let (Some(key), Some(this), Some(earlier)) = (key, info(epoch), info(before)) else {
                break;
            };
            let path = team_layout::key_history(epoch);
            let opened = self.read_repo(dir, &path)?.and_then(|sealed| {
                let plain = self.sync_crypto.open(&[key], &path, &sealed).ok()?;
                let signed: Signed = serde_json::from_slice(&plain).ok()?;
                // Signed by the PC that signed the epoch (an admin's of the
                // one before), and no other: a member who still holds the key
                // cannot feed a joiner a wrong one.
                let signer = earlier.members.iter().any(|m| {
                    m.id == this.signer
                        && m.seat.as_ref().is_some_and(|s| s.role == Role::Admin)
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
        self.edit_team_ids(&id, true)?;
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

    // ── People: removing, roles, leaving ────────────────────────────────

    /// Fails when the change would leave PCs in the team and no admin's
    /// among them: nobody could invite, remove or review any more.
    fn check_admins_stay(
        &self,
        space: &Space,
        removed: &[String],
        roles: &[(PersonId, Role)],
    ) -> AppResult<()> {
        let (_, members) = self.space_status(space)?.ok_or(AppError::SyncNotSetUp)?;
        let left: Vec<Seat> = members
            .into_iter()
            .filter(|m| !removed.iter().any(|r| r == m.device.id.as_str()))
            .filter_map(|m| m.seat)
            .map(|mut seat| {
                if let Some((_, role)) = roles.iter().find(|(p, _)| *p == seat.person) {
                    seat.role = *role;
                }
                seat
            })
            .collect();
        if !left.is_empty() && !left.iter().any(|s| s.role == Role::Admin) {
            return Err(AppError::TeamLastAdmin);
        }
        Ok(())
    }

    /// The files a removed PC wrote go (its batches, snapshots and name),
    /// except its `device.hedy`: a commit it has not pushed yet still
    /// rebases, and it then learns it was removed.
    fn drop_device_files(&self, dir: &Path, removed: &[String]) -> AppResult<()> {
        for device in removed {
            let device = DeviceId(device.clone());
            if !device.is_valid() {
                continue;
            }
            for path in self.sync_remote.files(dir, &layout::device_dir(&device))? {
                if path != layout::device_info(&device) {
                    self.sync_remote.remove(dir, &path)?;
                }
            }
        }
        Ok(())
    }

    /// This admin's PC answers for the newest versions `devices` wrote (all
    /// of them, or the team layer's notes only): the same content, signed
    /// here, so every PC still takes them once those PCs are out (or no
    /// longer admins). Bylines stay.
    fn take_over(&self, me: &Me, devices: &[String], team_layer_only: bool) -> AppResult<()> {
        if devices.is_empty() {
            return Ok(());
        }
        let space = &me.space;
        let mut outbox: Vec<Change> = self.sync_json(space, OUTBOX)?.unwrap_or_default();
        for item in self.store.sync_items(&space.id())? {
            let wrote = devices
                .iter()
                .any(|d| d == item.version.stamp.device.as_str());
            let team_layer = item.kind == ItemKind::Note && item.scope == Scope::Global;
            if !wrote || (team_layer_only && !team_layer) {
                continue;
            }
            let stamp = Stamp {
                device: me.device.clone(),
                ..item.version.stamp.clone()
            };
            let change = Change {
                version: item.version.restated(stamp),
                ..item
            };
            self.store.put_sync_item(&space.id(), &change)?;
            outbox.push(change);
        }
        self.set_sync_setting(space, OUTBOX, &json(&outbox))
    }

    /// An admin's change to the team's seats, now (after a sync: what they
    /// sent so far is in): a new epoch in a commit of its own, then a sync
    /// that sends what this PC took over.
    fn change_seats(
        &self,
        space: &Space,
        removed: &[String],
        roles: &[(PersonId, Role)],
        take_over: (&[String], bool),
    ) -> AppResult<()> {
        let me = self.team_me(space)?;
        if me.seat.role != Role::Admin {
            return Err(AppError::TeamNotAllowed);
        }
        self.check_admins_stay(space, removed, roles)?;
        let dir = self.sync_dir(space)?;
        // Nothing half written stays in the clone for the next sync to push.
        let wrote = (|| -> AppResult<()> {
            self.next_team_epoch(space, &me, &[], removed, roles)?;
            self.drop_device_files(&dir, removed)?;
            // Before the push: a crash after it must not lose the takeover
            // (restating the same content is harmless if the push fails).
            self.take_over(&me, take_over.0, take_over.1)?;
            Ok(self.sync_remote.push(&dir, "hedy team")?)
        })();
        if let Err(error) = wrote {
            let _ = self.sync_remote.discard_local(&dir);
            return Err(error);
        }
        self.sync_locked(space).map(drop)
    }

    /// The PCs of `person` in the team's trusted epoch.
    fn person_devices(&self, space: &Space, person: &PersonId) -> AppResult<Vec<String>> {
        let (_, members) = self.space_status(space)?.ok_or(AppError::SyncNotSetUp)?;
        Ok(members
            .into_iter()
            .filter(|m| m.seat.as_ref().is_some_and(|s| s.person == *person))
            .map(|m| m.device.id.0)
            .collect())
    }

    /// Removes a person and all their PCs (admins): the key rotates, so
    /// they read nothing written from now on; their notes stay under their
    /// name. Their access to the git repo is the host's: the app reminds the
    /// admin to take it away there.
    pub fn team_remove_person(&self, team: &str, person: &PersonId) -> AppResult<()> {
        let space = self.team_space(team)?;
        let _guard = self.sync_guard(&space)?;
        self.sync_locked(&space)?;
        let me = self.team_me(&space)?;
        if *person == me.seat.person {
            return Err(AppError::SyncRemoveThisPc);
        }
        let devices = self.person_devices(&space, person)?;
        if devices.is_empty() {
            return Err(AppError::TeamPersonNotFound(person.as_str().to_owned()));
        }
        self.change_seats(&space, &devices, &[], (&devices, false))
    }

    /// Gives a person another role (admins). Someone no longer an admin
    /// leaves notes in the team layer that this PC now answers for.
    pub fn team_set_role(&self, team: &str, person: &PersonId, role: Role) -> AppResult<()> {
        let space = self.team_space(team)?;
        let _guard = self.sync_guard(&space)?;
        self.sync_locked(&space)?;
        let (_, members) = self.space_status(&space)?.ok_or(AppError::SyncNotSetUp)?;
        let current = members
            .iter()
            .filter_map(|m| m.seat.as_ref())
            .find(|s| s.person == *person)
            .map(|s| s.role)
            .ok_or_else(|| AppError::TeamPersonNotFound(person.as_str().to_owned()))?;
        if current == role {
            return Ok(());
        }
        let devices = self.person_devices(&space, person)?;
        // Someone no longer an admin may have written what a member cannot
        // (the team layer, others' notes): this PC answers for all of it.
        let demoted: &[String] = if role == Role::Admin { &[] } else { &devices };
        self.change_seats(&space, &[], &[(person.clone(), role)], (demoted, false))
    }

    /// Removes one PC: an admin removes any; a member, their own (an
    /// admin's PC signs it on its next sync). This PC leaves instead.
    pub fn team_remove_pc(&self, team: &str, device: &DeviceId) -> AppResult<()> {
        let space = self.team_space(team)?;
        let _guard = self.sync_guard(&space)?;
        self.sync_locked(&space)?;
        let me = self.team_me(&space)?;
        if *device == me.device {
            return Err(AppError::SyncRemoveThisPc);
        }
        let (_, members) = self.space_status(&space)?.ok_or(AppError::SyncNotSetUp)?;
        let seat = members
            .iter()
            .find(|m| m.device.id == *device)
            .and_then(|m| m.seat.clone())
            .ok_or_else(|| AppError::SyncDeviceNotFound(device.as_str().to_owned()))?;
        let removed = [device.as_str().to_owned()];
        if me.seat.role == Role::Admin {
            return self.change_seats(&space, &removed, &[], (&removed, false));
        }
        if seat.person != me.seat.person {
            return Err(AppError::TeamNotAllowed);
        }
        self.ask_admin(
            &space,
            &me,
            &SeatRequest::Remove {
                device: device.clone(),
            },
        )
    }

    /// A member's PC asks an admin's PC to sign `request`.
    fn ask_admin(&self, space: &Space, me: &Me, request: &SeatRequest) -> AppResult<()> {
        let dir = self.sync_dir(space)?;
        let session = self.sync_session(space, dir.clone(), me.device.clone())?;
        let path = team_layout::seat_request(&me.device, &self.sync_crypto.random_hex(8)?);
        self.write_signed(&session, &path, &json(request))?;
        Ok(self.sync_remote.push(&dir, "hedy team")?)
    }

    /// A name for a new person: shown as typed but with any hidden
    /// character visible, and told apart from a name someone has already.
    fn new_name(
        &self,
        space: &Space,
        pending: &[(PersonId, String)],
        typed: &str,
    ) -> AppResult<String> {
        let roster = self.roster(space)?;
        let taken = |name: &str| {
            roster
                .values()
                .chain(pending.iter().map(|(_, n)| n))
                .any(|n| n.to_lowercase() == name.to_lowercase())
        };
        let base = shown_name(typed);
        let mut name = base.clone();
        let mut n = 2;
        while taken(&name) {
            name = format!("{base} ({n})");
            n += 1;
        }
        Ok(name)
    }

    /// Names (or renames) people: written in the admins' signed file on
    /// the next sync.
    fn name_people(&self, space: &Space, names: Vec<(PersonId, String)>) -> AppResult<()> {
        let mut roster = self.roster(space)?;
        let mut given: BTreeMap<PersonId, String> =
            self.sync_json(space, ROSTER_DUE)?.unwrap_or_default();
        for (person, name) in names {
            roster.insert(person.clone(), name.clone());
            given.insert(person, name);
        }
        self.set_sync_setting(space, ROSTER, &json(&roster))?;
        self.set_sync_setting(space, ROSTER_DUE, &json(&given))
    }

    /// Renames a person (admins): the name everyone sees on their notes.
    pub fn team_rename_person(&self, team: &str, person: &PersonId, name: &str) -> AppResult<()> {
        let space = self.team_space(team)?;
        let _guard = self.sync_guard(&space)?;
        if self.team_me(&space)?.seat.role != Role::Admin {
            return Err(AppError::TeamNotAllowed);
        }
        let name = name.trim();
        check_clean(name)?;
        if self.person_devices(&space, person)?.is_empty() {
            return Err(AppError::TeamPersonNotFound(person.as_str().to_owned()));
        }
        self.name_people(&space, vec![(person.clone(), name.to_owned())])
    }

    /// Renames the team (admins).
    pub fn team_rename(&self, team: &str, name: &str) -> AppResult<()> {
        let space = self.team_space(team)?;
        let _guard = self.sync_guard(&space)?;
        if self.team_me(&space)?.seat.role != Role::Admin {
            return Err(AppError::TeamNotAllowed);
        }
        let name = name.trim();
        check_clean(name)?;
        self.set_sync_setting(&space, NEW_NAME, name)
    }

    /// This PC leaves the team: it asks an admin's PC to take it out, then
    /// its keys, clone, settings and replica go here, and its projects'
    /// memory follows their owner again. The last admin of a team with
    /// other people cannot leave: they make someone else an admin first.
    pub fn team_leave(&self, team: &str) -> AppResult<()> {
        let space = self.team_space_or_joining(team)?;
        let _guard = self.sync_guard(&space)?;
        // A PC removed already just cleans up here.
        let me = match self.space_is_set_up(&space)? {
            true => match self.team_me(&space) {
                Ok(me) => Some(me),
                Err(AppError::SyncRemoved) => None,
                Err(error) => return Err(error),
            },
            false => None,
        };
        if let Some(me) = me {
            let mine = [me.device.as_str().to_owned()];
            self.check_admins_stay(&space, &mine, &[])?;
            let (_, members) = self.space_status(&space)?.ok_or(AppError::SyncNotSetUp)?;
            // Another PC takes this one out (an admin's, who also answers for
            // what it wrote). Without the network, it leaves here all the
            // same; the others remove it from their side.
            if !members.iter().all(|m| m.device.this_pc) {
                let asked = self.ask_admin(
                    &space,
                    &me,
                    &SeatRequest::Remove {
                        device: me.device.clone(),
                    },
                );
                // An admin's PC stays until it is out: two admins leaving
                // at once must not leave the team with none behind.
                if me.seat.role == Role::Admin {
                    asked?;
                    return self.set_sync_setting(&space, LEAVING, "1");
                }
            }
        }
        self.forget_team(&space)
    }

    /// This PC forgets the team: its keys, clone, settings and replica go,
    /// and its projects' memory follows their owner again.
    fn forget_team(&self, space: &Space) -> AppResult<()> {
        let Space::Team(id) = space else {
            unreachable!("a team's space");
        };
        let space = space.clone();
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
            ASKS,
            ROSTER,
            ROSTER_DUE,
            LEAVING,
        ] {
            self.clear_sync_setting(&space, key)?;
        }
        let team_home = MemoryHome::Team(id.clone());
        for project in self.store.list_projects()? {
            if self.store.memory_home(&project.path)? == Some(team_home.clone()) {
                self.store.set_memory_home(&project.path, None)?;
            }
        }
        self.edit_team_ids(id, false)
    }
}

/// A person's name as shown: trimmed, any hidden character visible, never
/// empty.
fn shown_name(typed: &str) -> String {
    let name = reveal_hidden(typed.trim());
    if name.is_empty() { "?".into() } else { name }
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
