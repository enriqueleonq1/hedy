//! Personal sync (docs/decisions.md §34): the same notes, skills, templates
//! and themes on every PC of the user, through a private git repo, sealed and
//! signed on this PC before they leave it.
//!
//! A sync is:
//! 1. pull;
//! 2. check the key epochs (the ones already checked are pinned by their
//!    hash; a new one must be signed by a member of the one before);
//! 3. finish a batch an earlier sync could not write;
//! 4. turn what changed here into versions: each local row is compared with
//!    the version it reflects (its link's base), so an edit made here, one
//!    made elsewhere and both are told apart;
//! 5. write this PC's batch (kept as pending first, so a failure loses
//!    nothing);
//! 6. apply the other PCs' new batches ([`merge`]), only from PCs that are
//!    members of the newest epoch and signed by them;
//! 7. bring in synced items this PC never had (a project made personal);
//! 8. push.
//!
//! Nothing here reads a login or an API key, and only projects whose memory
//! is personal take part.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::MutexGuard;

use serde::{Deserialize, Serialize};

use super::ports::{SyncConflict, SyncLink, SyncLock, TemplateRecord};
use super::{AppError, AppResult, Hedy};
use crate::domain::sync::{
    Batch, Change, DeviceId, EpochInfo, EpochMember, Former, ItemKind, MemoryHome, Merge, NoteBody,
    RECOVERY_MEMBER, RepoIdentity, Scope, SkillBody, Snapshot, Stamp, TemplateBody, ThemeBody,
    Version, layout, merge, url_has_credentials,
};
use crate::domain::team::Seat;
use crate::domain::theme::{CustomTheme, new_theme_id};
use crate::tr;

/// Settings of this PC's sync, per space ([`Space::key`]; plain, not
/// secret).
const URL: &str = "url";
const DEVICE: &str = "device";
const NAME: &str = "device_name";
/// The recovery key's public keys: the root every epoch is checked from.
const RECOVERY: &str = "recovery";
/// The newest epoch checked and trusted.
const EPOCH: &str = "epoch";
/// SHA-256 of every epoch file checked (JSON map): a checked epoch never
/// changes.
const PINS: &str = "pins";
/// This PC's last batch.
const SEQ: &str = "seq";
/// A batch decided but not yet written to the clone (JSON [`Pending`]).
const PENDING: &str = "pending";
/// Versions made outside a sync (a settled conflict), sent with the next.
const OUTBOX: &str = "outbox";
/// The newest stamp made or seen (JSON), so new ones are always later.
const CLOCK: &str = "clock";
const LAST_SYNC: &str = "last_sync";
/// The seq of this PC's own snapshot in the repo (0: none).
const SNAPSHOT: &str = "snapshot";
/// Why the last sync failed; cleared by the next one that works.
const LAST_ERROR: &str = "last_error";
/// When this PC last wrote its name and last sync to the repo.
const INFO_AT: &str = "info_at";

/// This PC writes its name and last sync at most this often (seconds), so
/// a sync with nothing new does not make a commit.
const INFO_EVERY: i64 = 3600;
/// A stamp further ahead than this (ms) does not move the clock: a PC with
/// a wrong date must not pin every PC's clock in the future.
const CLOCK_AHEAD_MAX: i64 = 24 * 3600 * 1000;
/// Files in the repo bigger than this are refused, not read.
const FILE_MAX: usize = 16 * 1024 * 1024;

/// Past this many batches of its own in the repo, a PC rewrites them as
/// one snapshot (fewer in tests, so every test also compacts).
const COMPACT_AFTER: usize = if cfg!(test) { 3 } else { 100 };

/// The former bodies a snapshot keeps of each item, newest last.
const FORMER_MAX: usize = 16;

const KINDS: [ItemKind; 4] = [
    ItemKind::Note,
    ItemKind::Skill,
    ItemKind::Template,
    ItemKind::Theme,
];

/// What a sync works on: personal sync, or a team (docs/decisions.md §35).
/// Each has its own clone, keys, settings and replica; a team is one more
/// space, synced by the same engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Space {
    Personal,
    /// A team, by its local id (16 hex).
    Team(String),
}

pub(super) const PERSONAL: &Space = &Space::Personal;

impl Space {
    /// How the store and the keyring name it: `""`, `team:<id>`.
    pub(super) fn id(&self) -> String {
        match self {
            Self::Personal => String::new(),
            Self::Team(id) => format!("team:{id}"),
        }
    }

    /// A setting of this space: personal sync's keep the names they had.
    pub(super) fn key(&self, name: &str) -> String {
        match self {
            Self::Personal => format!("sync.{name}"),
            Self::Team(id) => format!("team.{id}.{name}"),
        }
    }

    /// Its clone, inside Hedy's data folder `data`.
    fn dir(&self, data: &Path) -> PathBuf {
        match self {
            Self::Personal => data.join("sync").join("repo"),
            Self::Team(id) => data.join("teams").join(id).join("repo"),
        }
    }
}

/// What the keyring keeps: this PC's keys and every epoch key it has; in a
/// team, also its invites' secrets.
#[derive(Default, Serialize, Deserialize)]
struct Keys {
    device: String,
    epochs: BTreeMap<u32, String>,
    /// The invites this PC made, by number, until they are answered.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    invites: BTreeMap<u32, team::InviteKeys>,
    /// While this PC joins a team: the key its invite shares with the
    /// inviting PC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    invite: Option<String>,
}

/// A PC's name and last sync, as it writes them to the repo; in a team,
/// also its person's name.
#[derive(Serialize, Deserialize)]
struct DeviceInfo {
    name: String,
    last_sync: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    person_name: Option<String>,
}

/// What a PC writes, signed with its key: inside the seal, so only members
/// read it, and signed, so no member can write as another.
#[derive(Serialize, Deserialize)]
struct Signed {
    body: String,
    signature: String,
}

/// A version to send and the local row it comes from (to link).
#[derive(Clone, Serialize, Deserialize)]
struct Outgoing {
    change: Change,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    local_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Pending {
    seq: u64,
    outgoing: Vec<Outgoing>,
}

/// What one sync did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Changes made here and sent.
    pub sent: usize,
    /// Changes from other PCs applied here.
    pub received: usize,
    /// New conflicts for the user.
    pub conflicts: usize,
    /// Versions another PC wrote that this PC turned down (a team member
    /// writing what their role does not allow).
    pub refused: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncDevice {
    pub id: DeviceId,
    pub name: String,
    /// Seconds since the epoch; `None` until it writes it.
    pub last_sync: Option<i64>,
    pub this_pc: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncStatus {
    pub url: String,
    pub device: DeviceId,
    pub last_sync: Option<i64>,
    /// Why the last sync failed, until one works.
    pub last_error: Option<String>,
    /// The PCs of the trusted key epoch, this one first.
    pub devices: Vec<SyncDevice>,
    pub conflicts: usize,
}

/// A PC of a space's trusted epoch, as [`Hedy::space_status`] reads it.
pub(super) struct SpaceMember {
    pub(super) device: SyncDevice,
    /// In a team, whose PC it is.
    pub(super) seat: Option<Seat>,
    /// In a team, its person's name, as that PC wrote it.
    pub(super) person_name: Option<String>,
}

/// One local item as it would sync now.
struct LocalItem {
    kind: ItemKind,
    local_id: String,
    scope: Scope,
    body: String,
}

/// What a sync works with once it has the keys.
struct Session {
    space: Space,
    dir: PathBuf,
    device: DeviceId,
    /// This PC's secrets (to sign).
    me: String,
    /// Every epoch key this PC has.
    keys: Vec<String>,
    /// The newest: what this PC seals with.
    current: String,
    /// Sealed project id → the projects here with that remote.
    projects: HashMap<String, Vec<PathBuf>>,
    /// The projects that sync, and their sealed id (worked out once per
    /// sync: it reads each project's remotes).
    project_ids: HashMap<PathBuf, String>,
    /// The PCs of the newest epoch and their public keys.
    members: HashMap<String, String>,
    /// In a team, whose PC each of them is (from the same epoch).
    seats: HashMap<String, Seat>,
    /// In a team, what the other PCs' batches and snapshots hold, read
    /// first and applied together in the order they were made (a version
    /// is checked against the one it replaced, whichever PC wrote it).
    held: RefCell<Vec<(Change, bool)>>,
    /// In a team, how far each PC was read, saved once `held` is applied.
    read_to: RefCell<Vec<(DeviceId, u64)>>,
    /// On a PC's first sync, its rows not synced yet by what they hold: a
    /// synced item equal to one adopts that row instead of adding another.
    twins: RefCell<HashMap<(ItemKind, Scope, String), Vec<String>>>,
    /// The rows this first sync made, for an item's older version: a later
    /// version with a twin here moves the item onto the twin.
    made: RefCell<HashSet<(ItemKind, String)>>,
    /// What snapshots say their items held before (digests), so a first
    /// sync adopts a row equal to an older version.
    former: RefCell<HashMap<(ItemKind, String), Vec<String>>>,
    /// The twins' keys by the digest of what they hold (hashed once).
    twin_digests: RefCell<HashMap<String, (ItemKind, Scope, String)>>,
}

struct SyncGuard<'a> {
    _process: MutexGuard<'a, ()>,
    _machine: SyncLock,
}

/// The projects that sync here, both ways round (see [`Session`]).
struct SyncedProjects {
    projects: HashMap<String, Vec<PathBuf>>,
    project_ids: HashMap<PathBuf, String>,
}

/// The longest error kept for the status bar.
const ERROR_MAX: usize = 300;

/// An error as the app shows it: one line, without terminal escapes (a git
/// server's own `remote:` text can carry them), and short.
fn shown_error(error: &AppError) -> String {
    let text = error.to_string();
    let mut shown = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // `ESC [ … letter`, or a lone escape.
            if chars.next_if_eq(&'[').is_some() {
                while chars.next().is_some_and(|c| !c.is_ascii_alphabetic()) {}
            }
        } else if c.is_control() {
            shown.push(' ');
        } else {
            shown.push(c);
        }
    }
    let shown = shown.split_whitespace().collect::<Vec<_>>().join(" ");
    match shown.char_indices().nth(ERROR_MAX) {
        Some((at, _)) => format!("{}…", &shown[..at]),
        None => shown,
    }
}

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("sync records serialize")
}

fn corrupt(what: &str) -> AppError {
    AppError::Port(super::ports::PortError::new(tr!(
        "la sincronización encontró un archivo inválido ({what})",
        "sync found an invalid file ({what})"
    )))
}

/// What a signature covers: the path, then the body.
fn signed_bytes(path: &str, body: &str) -> Vec<u8> {
    [path.as_bytes(), b"\n", body.as_bytes()].concat()
}

impl Hedy {
    /// One sync operation at a time, in this process and on this PC (the
    /// app and `hedy`); a panic in another does not block the next.
    fn sync_guard(&self, space: &Space) -> AppResult<SyncGuard<'_>> {
        let process = self
            .sync_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let machine = self.sync_remote.lock(&self.sync_dir(space)?)?;
        Ok(SyncGuard {
            _process: process,
            _machine: machine,
        })
    }

    /// Whether anyone can read the repo at `url` without credentials. A
    /// public repo still syncs (all of it is encrypted), but anyone can see
    /// how many PCs sync and when; the app warns before using one.
    pub fn sync_repo_is_public(&self, url: &str) -> AppResult<bool> {
        if url_has_credentials(url) {
            return Err(AppError::SyncUrlWithCredentials);
        }
        Ok(self.sync_remote.is_public(url))
    }

    /// The name this PC takes when the user gives none: the machine's.
    pub fn default_device_name(&self) -> String {
        self.sync_remote.device_name()
    }

    pub(super) fn sync_setting(&self, space: &Space, name: &str) -> AppResult<Option<String>> {
        Ok(self.store.get_setting(&space.key(name))?)
    }

    pub(super) fn set_sync_setting(&self, space: &Space, name: &str, value: &str) -> AppResult<()> {
        Ok(self.store.set_setting(&space.key(name), Some(value))?)
    }

    pub(super) fn clear_sync_setting(&self, space: &Space, name: &str) -> AppResult<()> {
        Ok(self.store.set_setting(&space.key(name), None)?)
    }

    pub(super) fn sync_json<T: serde::de::DeserializeOwned>(
        &self,
        space: &Space,
        name: &str,
    ) -> AppResult<Option<T>> {
        Ok(self
            .sync_setting(space, name)?
            .and_then(|text| serde_json::from_str(&text).ok()))
    }

    fn sync_number(&self, space: &Space, name: &str) -> AppResult<u64> {
        Ok(self
            .sync_setting(space, name)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0))
    }

    /// A space's clone, inside Hedy's data folder (never a relative path: a
    /// failed setup deletes it).
    pub(super) fn sync_dir(&self, space: &Space) -> AppResult<PathBuf> {
        let dir = self
            .store
            .data_dir()
            .and_then(|dir| std::path::absolute(dir).ok())
            .ok_or_else(|| corrupt("data dir"))?;
        Ok(space.dir(&dir))
    }

    pub fn sync_is_set_up(&self) -> AppResult<bool> {
        self.space_is_set_up(PERSONAL)
    }

    pub(super) fn space_is_set_up(&self, space: &Space) -> AppResult<bool> {
        Ok(self.sync_setting(space, URL)?.is_some())
    }

    fn keys(&self, space: &Space) -> AppResult<Keys> {
        let text = self
            .secrets
            .sync_keys(&space.id())?
            .ok_or(AppError::SyncNotSetUp)?;
        serde_json::from_str(&text).map_err(|_| corrupt("keyring"))
    }

    fn save_keys(&self, space: &Space, keys: &Keys) -> AppResult<()> {
        Ok(self.secrets.set_sync_keys(&space.id(), &json(keys))?)
    }

    /// A file of the repo, refused when it is too big to be Hedy's.
    fn read_repo(&self, dir: &Path, path: &str) -> AppResult<Option<Vec<u8>>> {
        let bytes = self.sync_remote.read(dir, path)?;
        if bytes.as_ref().is_some_and(|b| b.len() > FILE_MAX) {
            return Err(corrupt(path));
        }
        Ok(bytes)
    }

    /// The epoch files in the repo, by number. Files with other names are
    /// not Hedy's and are left alone.
    fn epochs(&self, dir: &Path) -> AppResult<Vec<(u32, String)>> {
        let mut epochs = Vec::new();
        for path in self.sync_remote.files(dir, layout::EPOCHS_DIR)? {
            let Some(number) = path
                .strip_prefix(layout::EPOCHS_DIR)
                .and_then(|p| p.strip_prefix('/'))
                .and_then(|p| p.strip_suffix(".json"))
                .filter(|n| n.len() == 6 && n.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            let Some(bytes) = self.read_repo(dir, &path)? else {
                continue;
            };
            // Not an epoch file (junk, or one a newer Hedy wrote): left alone.
            let Ok(text) = String::from_utf8(bytes) else {
                continue;
            };
            match self.sync_crypto.epoch_members(&text) {
                Ok(info) if info.number == number => epochs.push((number, text)),
                _ => continue,
            }
        }
        epochs.sort_by_key(|(n, _)| *n);
        Ok(epochs)
    }

    /// Starts syncing with the empty repo at `url`: this PC's keys, a new
    /// recovery key and the first epoch, then a first sync. Returns the 12
    /// words, to show the user once. Nothing is kept here until the repo
    /// accepted the first epoch.
    pub fn sync_setup(&self, url: &str, device_name: &str) -> AppResult<String> {
        let space = PERSONAL;
        let _guard = self.sync_guard(space)?;
        self.starting_sync(space, url, || self.setup_locked(url, device_name))
    }

    /// Runs a setup or a join; one that fails before the sync is set up
    /// leaves no clone behind (it would hold the folder for another URL).
    fn starting_sync<T>(
        &self,
        space: &Space,
        url: &str,
        start: impl FnOnce() -> AppResult<T>,
    ) -> AppResult<T> {
        if self.space_is_set_up(space)? {
            return Err(AppError::SyncAlreadySetUp);
        }
        if url_has_credentials(url) {
            return Err(AppError::SyncUrlWithCredentials);
        }
        let result = start();
        if result.is_err() && !self.space_is_set_up(space)? {
            let _ = self.sync_remote.discard_local(&self.sync_dir(space)?);
        }
        result
    }

    fn setup_locked(&self, url: &str, device_name: &str) -> AppResult<String> {
        let space = PERSONAL;
        let dir = self.sync_dir(space)?;
        self.sync_remote.open(url, &dir)?;
        if !self.epochs(&dir)?.is_empty() {
            return Err(AppError::SyncRepoInUse);
        }
        // Not someone's code (a mistyped address): it would be written into.
        if !self.sync_remote.files(&dir, "")?.is_empty() {
            return Err(AppError::SyncRepoNotEmpty);
        }
        let device = DeviceId(self.sync_crypto.random_hex(8)?);
        let me = self.sync_crypto.new_member()?;
        let (words, recovery) = self.sync_crypto.new_recovery()?;
        let recovery_public = self.sync_crypto.member_public(&recovery)?;
        let members = [
            EpochMember::new(RECOVERY_MEMBER, recovery_public.clone()),
            EpochMember::new(device.as_str(), self.sync_crypto.member_public(&me)?),
        ];
        let (file, key) =
            self.sync_crypto
                .new_epoch(1, false, &members, RECOVERY_MEMBER, &recovery)?;
        self.sync_remote
            .write(&dir, &layout::epoch(1), file.as_bytes())?;
        self.sync_remote
            .write(&dir, layout::README, readme().as_bytes())?;
        let keys = Keys {
            device: me,
            epochs: BTreeMap::from([(1, key)]),
            ..Keys::default()
        };
        self.publish_epoch(space, &dir, &keys)?;
        let pins = BTreeMap::from([(1, self.sync_crypto.digest(file.as_bytes()))]);
        self.remember_sync(space, url, &device, device_name, &recovery_public, 1, &pins)?;
        // The sync is set up: the words must reach the user even when this
        // first sync fails (the next one sends what is left).
        let _ = self.sync_locked(space);
        Ok(words)
    }

    /// Joins the sync at `url` with the recovery key's `words`: checks every
    /// epoch from the recovery key, adds this PC in a new epoch, then syncs
    /// (which brings everything in).
    pub fn sync_join(&self, url: &str, words: &str, device_name: &str) -> AppResult<SyncReport> {
        let space = PERSONAL;
        let _guard = self.sync_guard(space)?;
        self.starting_sync(space, url, || self.join_locked(url, words, device_name))
    }

    fn join_locked(&self, url: &str, words: &str, device_name: &str) -> AppResult<SyncReport> {
        let space = PERSONAL;
        let dir = self.sync_dir(space)?;
        self.sync_remote.open(url, &dir)?;
        let epochs = self.epochs(&dir)?;
        let Some((first_number, first)) = epochs.first() else {
            return Err(AppError::SyncRepoEmpty);
        };
        let recovery = self.sync_crypto.recover(words)?;
        let recovery_public = self.sync_crypto.member_public(&recovery)?;
        if *first_number != 1 {
            return Err(corrupt(&layout::epoch(*first_number)));
        }
        self.sync_crypto
            .verify_first_epoch(first, &recovery_public)?;
        let mut epoch_keys = BTreeMap::new();
        let mut pins = BTreeMap::new();
        let mut trusted = 0;
        for (i, (number, file)) in epochs.iter().enumerate() {
            // The chain ends at one that does not follow (as in a sync).
            if i > 0
                && self
                    .sync_crypto
                    .verify_next_epoch(&epochs[i - 1].1, file)
                    .is_err()
            {
                break;
            }
            epoch_keys.insert(
                *number,
                self.sync_crypto
                    .open_epoch(file, RECOVERY_MEMBER, &recovery)?,
            );
            pins.insert(*number, self.sync_crypto.digest(file.as_bytes()));
            trusted = i;
        }
        let (last, last_file) = &epochs[trusted];
        let device = DeviceId(self.sync_crypto.random_hex(8)?);
        let me = self.sync_crypto.new_member()?;
        let EpochInfo {
            team, mut members, ..
        } = self.sync_crypto.epoch_members(last_file)?;
        // A team's PCs join with an invite, as a person (§35).
        if team {
            return Err(AppError::SyncTeamRepo);
        }
        members.push(EpochMember::new(
            device.as_str(),
            self.sync_crypto.member_public(&me)?,
        ));
        let next = last + 1;
        let (file, key) =
            self.sync_crypto
                .new_epoch(next, false, &members, RECOVERY_MEMBER, &recovery)?;
        self.sync_remote
            .write(&dir, &layout::epoch(next), file.as_bytes())?;
        epoch_keys.insert(next, key);
        pins.insert(next, self.sync_crypto.digest(file.as_bytes()));
        let keys = Keys {
            device: me,
            epochs: epoch_keys,
            ..Keys::default()
        };
        self.publish_epoch(space, &dir, &keys)?;
        self.remember_sync(
            space,
            url,
            &device,
            device_name,
            &recovery_public,
            next,
            &pins,
        )?;
        self.sync_locked(space)
    }

    /// Keeps this PC's keys and pushes the new epoch; when the repo refuses
    /// it (another PC wrote the same epoch first), forgets both, so a retry
    /// starts clean.
    fn publish_epoch(&self, space: &Space, dir: &Path, keys: &Keys) -> AppResult<()> {
        self.save_keys(space, keys)?;
        if let Err(error) = self.sync_remote.push(dir, "hedy sync") {
            let _ = self.secrets.delete_sync_keys(&space.id());
            let _ = self.sync_remote.discard_local(dir);
            return Err(error.into());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn remember_sync(
        &self,
        space: &Space,
        url: &str,
        device: &DeviceId,
        name: &str,
        recovery_public: &str,
        epoch: u32,
        pins: &BTreeMap<u32, String>,
    ) -> AppResult<()> {
        self.set_sync_setting(space, DEVICE, device.as_str())?;
        self.set_sync_setting(space, NAME, name.trim())?;
        self.set_sync_setting(space, RECOVERY, recovery_public)?;
        self.set_sync_setting(space, EPOCH, &epoch.to_string())?;
        self.set_sync_setting(space, PINS, &json(pins))?;
        self.set_sync_setting(space, SEQ, "0")?;
        // Last: what says the sync is set up.
        self.set_sync_setting(space, URL, url)
    }

    /// One sync, now.
    pub fn sync_now(&self) -> AppResult<SyncReport> {
        let space = PERSONAL;
        let _guard = self.sync_guard(space)?;
        let result = self.sync_locked(space);
        if self.space_is_set_up(space)? {
            match &result {
                Ok(_) => self.clear_sync_setting(space, LAST_ERROR)?,
                Err(error) => self.set_sync_setting(space, LAST_ERROR, &shown_error(error))?,
            }
        }
        result
    }

    fn sync_locked(&self, space: &Space) -> AppResult<SyncReport> {
        let url = self
            .sync_setting(space, URL)?
            .ok_or(AppError::SyncNotSetUp)?;
        let device = DeviceId(
            self.sync_setting(space, DEVICE)?
                .ok_or(AppError::SyncNotSetUp)?,
        );
        let dir = self.sync_dir(space)?;
        self.sync_remote.open(&url, &dir)?;
        let session = self.sync_session(space, dir, device)?;
        let mut report = SyncReport::default();

        self.write_pending(&session)?;
        let personal = *space == Space::Personal;
        // A PC's first sync (a join, or after leaving) takes in what the repo
        // has before looking for changes here: rows it already has are
        // adopted, not sent again as new items.
        if personal && self.store.sync_items(&space.id())?.is_empty() {
            self.index_twins(&session)?;
            self.apply_batches(&session, &mut report)?;
            self.bring_waiting(&session)?;
            session.twins.borrow_mut().clear();
            session.made.borrow_mut().clear();
            session.twin_digests.borrow_mut().clear();
        }
        // What changed here first, so the other PCs' versions meet it.
        let mut outgoing: Vec<Outgoing> = self
            .sync_json::<Vec<Change>>(space, OUTBOX)?
            .unwrap_or_default()
            .into_iter()
            .map(|change| Outgoing {
                change,
                local_id: None,
            })
            .collect();
        // A team's notes change through its own use cases (`team`), which
        // leave their versions in the outbox.
        if personal {
            outgoing.extend(self.local_changes(&session, &mut report)?);
        }
        report.sent = outgoing.len();
        if !outgoing.is_empty() {
            let seq = self.sync_number(space, SEQ)? + 1;
            self.set_sync_setting(space, PENDING, &json(&Pending { seq, outgoing }))?;
            self.set_sync_setting(space, SEQ, &seq.to_string())?;
            self.clear_sync_setting(space, OUTBOX)?;
            self.write_pending(&session)?;
        }
        self.compact(&session)?;
        // This PC's batch leaves even when another PC's cannot be applied.
        let applied = self.apply_batches(&session, &mut report).and_then(|()| {
            if personal {
                self.bring_waiting(&session)
            } else {
                self.team_duties(&session)
            }
        });

        let now = self.now_secs();
        let info_at = self.sync_number(space, INFO_AT)? as i64;
        if report.sent > 0 || now - info_at >= INFO_EVERY {
            let info = DeviceInfo {
                name: self.sync_setting(space, NAME)?.unwrap_or_default(),
                last_sync: now,
                person_name: self.sync_setting(space, team::PERSON_NAME)?,
            };
            self.write_signed(
                &session,
                &layout::device_info(&session.device),
                &json(&info),
            )?;
            self.set_sync_setting(space, INFO_AT, &now.to_string())?;
        }
        self.sync_remote.push(&session.dir, "hedy sync")?;
        applied?;
        self.set_sync_setting(space, LAST_SYNC, &now.to_string())?;
        Ok(report)
    }

    /// Seals `body` for `path`, signed by this PC, and writes it.
    fn write_signed(&self, session: &Session, path: &str, body: &str) -> AppResult<()> {
        let signature = self
            .sync_crypto
            .sign(&session.me, &signed_bytes(path, body))?;
        let envelope = json(&Signed {
            body: body.to_owned(),
            signature,
        });
        let sealed = self
            .sync_crypto
            .seal(&session.current, path, envelope.as_bytes())?;
        Ok(self.sync_remote.write(&session.dir, path, &sealed)?)
    }

    /// Opens a file `device` wrote: sealed with one of the keys, signed by
    /// that PC as a member of the newest epoch. `None`: not a member (a
    /// removed PC, or a forged one).
    fn open_signed(
        &self,
        session: &Session,
        device: &DeviceId,
        path: &str,
    ) -> AppResult<Option<String>> {
        let Some(public) = session.members.get(device.as_str()) else {
            return Ok(None);
        };
        let Some(sealed) = self.read_repo(&session.dir, path)? else {
            return Ok(None);
        };
        let plain = self.sync_crypto.open(&session.keys, path, &sealed)?;
        let signed: Signed = serde_json::from_slice(&plain).map_err(|_| corrupt(path))?;
        self.sync_crypto
            .verify(public, &signed_bytes(path, &signed.body), &signed.signature)?;
        Ok(Some(signed.body))
    }

    /// Writes the pending batch (if any) and records what it sends: the
    /// replica, and the links of the local rows it comes from.
    fn write_pending(&self, session: &Session) -> AppResult<()> {
        let space = &session.space;
        let Some(pending) = self.sync_json::<Pending>(space, PENDING)? else {
            return Ok(());
        };
        // Saved before SEQ moved (a crash in between): never reuse its seq.
        if self.sync_number(space, SEQ)? < pending.seq {
            self.set_sync_setting(space, SEQ, &pending.seq.to_string())?;
        }
        for out in &pending.outgoing {
            let change = &out.change;
            self.store.put_sync_item(&space.id(), change)?;
            match (&out.local_id, change.version.is_deletion()) {
                (Some(_), true) => self.store.unlink_sync_item(change.kind, &change.uid)?,
                (Some(local), false) => {
                    self.store
                        .link_sync_item(change.kind, &change.uid, local, &change.version)?
                }
                (None, _) => {}
            }
        }
        let batch = Batch {
            device: session.device.clone(),
            seq: pending.seq,
            changes: pending.outgoing.into_iter().map(|o| o.change).collect(),
        };
        self.write_signed(
            session,
            &layout::batch(&session.device, batch.seq),
            &json(&batch),
        )?;
        self.clear_sync_setting(space, PENDING)
    }

    /// Checks the key epochs: every one already checked must be unchanged
    /// (its pinned hash), and any newer one signed by a member of the one
    /// before. Keeps the new keys; fails when the newest leaves this PC out.
    fn sync_session(&self, space: &Space, dir: PathBuf, device: DeviceId) -> AppResult<Session> {
        let mut keys = self.keys(space)?;
        let trusted = self.sync_number(space, EPOCH)? as u32;
        let mut pins = self.pins(space)?;
        let recovery = self.sync_setting(space, RECOVERY)?.unwrap_or_default();
        let epochs = self.epochs(&dir)?;
        for (number, file) in epochs.iter().filter(|(n, _)| *n <= trusted) {
            if pins
                .get(number)
                .is_some_and(|pin| *pin != self.sync_crypto.digest(file.as_bytes()))
            {
                return Err(AppError::SyncTampered(*number));
            }
        }
        let Some(mut previous) = epochs
            .iter()
            .find(|(n, _)| *n == trusted)
            .map(|(_, f)| f.clone())
        else {
            return Err(corrupt(&layout::epoch(trusted)));
        };
        pins.entry(trusted)
            .or_insert_with(|| self.sync_crypto.digest(previous.as_bytes()));
        let mut newest = trusted;
        for (number, file) in epochs.iter().filter(|(n, _)| *n > trusted) {
            // One that does not follow (forged by someone with push access,
            // or a member trying to add one) is not trusted, and neither is
            // anything after it. Ignoring it, instead of stopping, keeps a
            // stolen PC from halting every sync; a PC that removes it
            // writes over it.
            if self.sync_crypto.verify_next_epoch(&previous, file).is_err() {
                break;
            }
            // The recovery key stays a member (it is how a PC joins); one
            // without it is not trusted either.
            let members = self.sync_crypto.epoch_members(file)?.members;
            if !members
                .iter()
                .any(|m| m.id == RECOVERY_MEMBER && m.public == recovery)
            {
                break;
            }
            pins.insert(*number, self.sync_crypto.digest(file.as_bytes()));
            match self
                .sync_crypto
                .open_epoch(file, device.as_str(), &keys.device)
            {
                Ok(key) => {
                    keys.epochs.insert(*number, key);
                }
                // Left out of this epoch: removed (unless a later one adds it back).
                Err(_) => {
                    keys.epochs.remove(number);
                }
            }
            previous = file.clone();
            newest = *number;
        }
        // A team's PC reads what was sealed before it joined.
        let older = matches!(space, Space::Team(_))
            && self.fill_older_keys(&dir, &epochs, newest, &mut keys)?;
        if newest != trusted || older {
            self.save_keys(space, &keys)?;
            self.set_sync_setting(space, EPOCH, &newest.to_string())?;
        }
        self.set_sync_setting(space, PINS, &json(&pins))?;
        let current = keys
            .epochs
            .get(&newest)
            .cloned()
            .ok_or(AppError::SyncRemoved)?;
        let members = self.sync_crypto.epoch_members(&previous)?.members;
        // A team names projects with epoch 1's key, the same on every PC;
        // without it, no project of the team is matched here.
        let SyncedProjects {
            projects,
            project_ids,
        } = match space {
            Space::Personal => {
                let name_key = keys
                    .epochs
                    .values()
                    .next()
                    .cloned()
                    .ok_or(AppError::SyncRemoved)?;
                self.synced_projects(space, &name_key)?
            }
            Space::Team(_) => match keys.epochs.get(&1) {
                Some(name_key) => self.synced_projects(space, name_key)?,
                None => SyncedProjects {
                    projects: HashMap::new(),
                    project_ids: HashMap::new(),
                },
            },
        };
        Ok(Session {
            space: space.clone(),
            dir,
            device,
            me: keys.device.clone(),
            keys: keys.epochs.into_values().collect(),
            current,
            projects,
            project_ids,
            seats: members
                .iter()
                .filter_map(|m| Some((m.id.clone(), m.seat.clone()?)))
                .collect(),
            members: members.into_iter().map(|m| (m.id, m.public)).collect(),
            twins: RefCell::default(),
            made: RefCell::default(),
            former: RefCell::default(),
            twin_digests: RefCell::default(),
            held: RefCell::default(),
            read_to: RefCell::default(),
        })
    }

    /// The pinned epoch hashes. A setting that does not read fails closed:
    /// nothing would be checked otherwise.
    fn pins(&self, space: &Space) -> AppResult<BTreeMap<u32, String>> {
        match self.sync_setting(space, PINS)? {
            Some(text) => serde_json::from_str(&text).map_err(|_| corrupt(PINS)),
            None => Ok(BTreeMap::new()),
        }
    }

    /// The projects whose memory is personal here, by their sealed id (of
    /// `origin`'s `owner/name`, what matches them on other PCs). Two
    /// projects here with the same remote share one.
    fn synced_projects(&self, space: &Space, name_key: &str) -> AppResult<SyncedProjects> {
        let mut projects: HashMap<String, Vec<PathBuf>> = HashMap::new();
        let mut project_ids = HashMap::new();
        for project in self.store.list_projects()? {
            let ours = |home: MemoryHome| match space {
                Space::Personal => home.syncs(),
                Space::Team(id) => home == MemoryHome::Team(id.clone()),
            };
            if !self.effective_memory_home(&project.path)?.is_some_and(ours) {
                continue;
            }
            let url = self
                .source
                .remote_url(&project.path, "origin")
                .ok()
                .flatten();
            let Some(identity) = url.as_deref().and_then(RepoIdentity::from_url) else {
                continue;
            };
            let id = self
                .sync_crypto
                .project_id(name_key, &identity.project_key())?;
            projects
                .entry(id.clone())
                .or_default()
                .push(project.path.clone());
            project_ids.insert(project.path, id);
        }
        Ok(SyncedProjects {
            projects,
            project_ids,
        })
    }

    /// The scope a local item syncs under; `None` when it stays here.
    fn scope_of(session: &Session, project: Option<&Path>) -> Option<Scope> {
        match project {
            None => Some(Scope::Global),
            Some(path) => session
                .project_ids
                .get(path)
                .map(|id| Scope::Project(id.clone())),
        }
    }

    /// Whether items of `scope` sync on this PC now.
    fn scope_here(session: &Session, scope: &Scope) -> bool {
        match scope {
            Scope::Global => true,
            Scope::Project(id) => session.projects.contains_key(id),
        }
    }

    /// Every local item that syncs, as it is now.
    fn local_items(&self, session: &Session) -> AppResult<Vec<LocalItem>> {
        let mut items = Vec::new();
        let mut places: Vec<Option<PathBuf>> = vec![None];
        places.extend(
            self.store
                .list_projects()?
                .into_iter()
                .map(|p| Some(p.path)),
        );
        for place in &places {
            let Some(scope) = Self::scope_of(session, place.as_deref()) else {
                continue;
            };
            for note in self.store.notes(place.as_deref(), false)? {
                let body = NoteBody {
                    text: note.text,
                    category: note.category,
                    author: note.author,
                    status: note.status,
                    created_at: note.created_at,
                    topic: note.topic,
                    updated_at: note.updated_at,
                };
                items.push(LocalItem {
                    kind: ItemKind::Note,
                    local_id: note.id.to_string(),
                    scope: scope.clone(),
                    body: json(&body),
                });
            }
            for skill in self.store.skills(place.as_deref(), false)? {
                let body = SkillBody {
                    name: skill.name,
                    description: skill.description,
                    body: skill.body,
                    status: skill.status,
                    author: skill.author,
                    created_at: skill.created_at,
                    updated_at: skill.updated_at,
                };
                items.push(LocalItem {
                    kind: ItemKind::Skill,
                    local_id: skill.id.to_string(),
                    scope: scope.clone(),
                    body: json(&body),
                });
            }
        }
        for record in self.store.template_records()? {
            let Some(scope) = Self::scope_of(session, record.project.as_deref()) else {
                continue;
            };
            let body = TemplateBody {
                name: record.name,
                text: record.text,
                builtin: record.builtin,
                deleted: record.deleted,
            };
            items.push(LocalItem {
                kind: ItemKind::Template,
                local_id: record.id.to_string(),
                scope,
                body: json(&body),
            });
        }
        for theme in self.themes.list()? {
            let body = ThemeBody {
                name: theme.name,
                base: theme.base,
                colors: theme.colors,
                origin: theme.origin,
            };
            items.push(LocalItem {
                kind: ItemKind::Theme,
                local_id: theme.id,
                scope: Scope::Global,
                body: json(&body),
            });
        }
        Ok(items)
    }

    /// Whether the local item is still here (whatever its scope). A theme
    /// this version cannot read (from a newer Hedy) is still there.
    fn local_exists(&self, kind: ItemKind, local_id: &str) -> AppResult<bool> {
        let row = local_id.parse::<i64>().ok();
        Ok(match kind {
            ItemKind::Note => match row {
                Some(id) => self.store.note(id)?.is_some(),
                None => false,
            },
            ItemKind::Skill => match row {
                Some(id) => self.store.skill(id)?.is_some(),
                None => false,
            },
            ItemKind::Template => self
                .store
                .template_records()?
                .iter()
                .any(|r| Some(r.id) == row),
            ItemKind::Theme => !matches!(self.themes.get(local_id), Ok(None)),
            ItemKind::Suggestion => false,
        })
    }

    /// A new uid for a local item. An edited built-in template gets one made
    /// from its key and scope, so the same built-in edited on two PCs is one
    /// item.
    fn new_uid(&self, item: &LocalItem) -> AppResult<String> {
        if item.kind == ItemKind::Template
            && let Ok(body) = serde_json::from_str::<TemplateBody>(&item.body)
            && let Some(builtin) = body.builtin
        {
            let key = format!("{builtin}\n{}", json(&item.scope));
            return Ok(format!(
                "builtin-{}",
                &self.sync_crypto.digest(key.as_bytes())[..32]
            ));
        }
        Ok(self.sync_crypto.random_hex(16)?)
    }

    /// A new stamp, later than any made or seen here.
    fn next_stamp(&self, space: &Space, device: &DeviceId) -> AppResult<Stamp> {
        let last: Option<Stamp> = self.sync_json(space, CLOCK)?;
        let stamp = Stamp::next(last.as_ref(), self.clock.now_ms(), device);
        self.set_sync_setting(space, CLOCK, &json(&stamp))?;
        Ok(stamp)
    }

    /// Keeps the clock ahead of a stamp from another PC (not one far in the
    /// future: a PC with a wrong date).
    fn observe_stamp(&self, space: &Space, seen: &Stamp) -> AppResult<()> {
        if seen.ms > self.clock.now_ms() + CLOCK_AHEAD_MAX {
            return Ok(());
        }
        let last: Option<Stamp> = self.sync_json(space, CLOCK)?;
        if last.as_ref().is_none_or(|l| seen > l) {
            self.set_sync_setting(space, CLOCK, &json(seen))?;
        }
        Ok(())
    }

    /// What changed here since each row last matched its synced version:
    /// new items, edits (a conflict when the item also changed elsewhere and
    /// this row had not caught up), and deletions. A row that only fell
    /// behind (its project was not synced for a while) catches up instead.
    fn local_changes(
        &self,
        session: &Session,
        report: &mut SyncReport,
    ) -> AppResult<Vec<Outgoing>> {
        let space = &session.space;
        let mut out = Vec::new();
        for item in self.local_items(session)? {
            let link = self.store.sync_link_of(item.kind, &item.local_id)?;
            let known = match &link {
                Some(link) => self.store.sync_item(&space.id(), item.kind, &link.uid)?,
                None => None,
            };
            let (link, (known_scope, known)) = match (link, known) {
                (Some(link), Some(known)) => (link, known),
                (link, _) => {
                    if let Some(link) = link {
                        self.store.unlink_sync_item(item.kind, &link.uid)?;
                    }
                    if let Some(new) = self.new_item(session, item)? {
                        out.push(new);
                    }
                    continue;
                }
            };
            // SQLite gives a deleted last row's id to the next row: another
            // birth means another item, so the linked one was deleted here.
            if let Some(base) = &link.base_body
                && birth(item.kind, base) != birth(item.kind, &item.body)
            {
                self.store.unlink_sync_item(item.kind, &link.uid)?;
                if !known.is_deletion() && Self::scope_here(session, &known_scope) {
                    let stamp = self.next_stamp(space, &session.device)?;
                    out.push(Outgoing {
                        change: Change {
                            kind: item.kind,
                            uid: link.uid.clone(),
                            scope: known_scope,
                            version: known.replaced(stamp, None),
                        },
                        local_id: None,
                    });
                }
                if let Some(new) = self.new_item(session, item)? {
                    out.push(new);
                }
                continue;
            }
            // Only the body: a project whose remote changed on one PC keeps
            // its items where they are.
            let edited_here = link.base_body.as_deref() != Some(item.body.as_str());
            let changed_elsewhere = known.stamp != link.base_stamp;
            if !edited_here {
                if changed_elsewhere {
                    let change = Change {
                        kind: item.kind,
                        uid: link.uid,
                        scope: known_scope,
                        version: known,
                    };
                    self.materialize(session, &change)?;
                }
                continue;
            }
            let stamp = self.next_stamp(space, &session.device)?;
            let version = known.replaced(stamp, Some(item.body.clone()));
            // Edited here while it changed (or was deleted) elsewhere.
            if changed_elsewhere
                && item.kind.asks_on_conflict()
                && known.body.as_deref() != Some(item.body.as_str())
            {
                report.conflicts +=
                    self.record_conflict(space, item.kind, &link.uid, &version, &known)?;
            }
            out.push(Outgoing {
                change: Change {
                    kind: item.kind,
                    uid: link.uid,
                    scope: known_scope,
                    version,
                },
                local_id: Some(item.local_id),
            });
        }
        for kind in KINDS {
            for link in self.store.sync_links(kind)? {
                if self.local_exists(kind, &link.local_id)? {
                    continue;
                }
                let Some((scope, known)) = self.store.sync_item(&space.id(), kind, &link.uid)?
                else {
                    self.store.unlink_sync_item(kind, &link.uid)?;
                    continue;
                };
                // A project not synced here now deletes nothing elsewhere.
                if !Self::scope_here(session, &scope) {
                    continue;
                }
                if known.is_deletion() {
                    self.store.unlink_sync_item(kind, &link.uid)?;
                    continue;
                }
                let stamp = self.next_stamp(space, &session.device)?;
                let version = known.replaced(stamp, None);
                if known.stamp != link.base_stamp && kind.asks_on_conflict() {
                    report.conflicts +=
                        self.record_conflict(space, kind, &link.uid, &version, &known)?;
                }
                out.push(Outgoing {
                    change: Change {
                        kind,
                        uid: link.uid,
                        scope,
                        version,
                    },
                    local_id: Some(link.local_id),
                });
            }
        }
        Ok(out)
    }

    /// Records a conflict for the user, once: a sync that is retried meets
    /// the same two versions again. Returns how many were recorded (0 or 1).
    fn record_conflict(
        &self,
        space: &Space,
        kind: ItemKind,
        uid: &str,
        kept: &Version,
        other: &Version,
    ) -> AppResult<usize> {
        let known = self
            .store
            .sync_conflicts(&space.id())?
            .into_iter()
            .any(|c| {
                c.kind == kind
                    && c.uid == uid
                    && c.kept.stamp == kept.stamp
                    && c.other.stamp == other.stamp
            });
        if known {
            return Ok(0);
        }
        self.store
            .add_sync_conflict(&space.id(), kind, uid, kept, other, self.now_secs())?;
        Ok(1)
    }

    /// A local item not synced yet. `None` when the replica already holds
    /// it as it is here (a built-in edited the same way elsewhere): it is
    /// only linked.
    fn new_item(&self, session: &Session, item: LocalItem) -> AppResult<Option<Outgoing>> {
        let space = &session.space;
        let uid = self.new_uid(&item)?;
        let version = match self.store.sync_item(&space.id(), item.kind, &uid)? {
            Some((_, known)) if known.body.as_deref() == Some(item.body.as_str()) => {
                self.store
                    .link_sync_item(item.kind, &uid, &item.local_id, &known)?;
                return Ok(None);
            }
            Some((_, known)) => {
                known.replaced(self.next_stamp(space, &session.device)?, Some(item.body))
            }
            None => Version::new(self.next_stamp(space, &session.device)?, Some(item.body)),
        };
        Ok(Some(Outgoing {
            change: Change {
                kind: item.kind,
                uid,
                scope: item.scope,
                version,
            },
            local_id: Some(item.local_id),
        }))
    }

    /// Reads the batches of the newest epoch's PCs not read yet, in order.
    /// A PC not in it (removed, or never added) is not read. When the next
    /// batch is gone (compacted), its snapshot stands for it.
    fn apply_batches(&self, session: &Session, report: &mut SyncReport) -> AppResult<()> {
        let space = &session.space;
        let mut pending: BTreeMap<DeviceId, Vec<(u64, String)>> = BTreeMap::new();
        let mut snapshots: HashMap<DeviceId, Vec<(u64, String)>> = HashMap::new();
        for path in self.sync_remote.files(&session.dir, layout::DEVICES_DIR)? {
            let member = |device: &DeviceId| {
                *device != session.device && session.members.contains_key(device.as_str())
            };
            if let Some((device, seq)) = layout::batch_seq(&path)
                && member(&device)
            {
                pending.entry(device).or_default().push((seq, path));
            } else if let Some((device, seq)) = layout::snapshot_seq(&path)
                && member(&device)
            {
                pending.entry(device.clone()).or_default();
                snapshots.entry(device).or_default().push((seq, path));
            }
        }
        for (device, mut batches) in pending {
            batches.sort();
            let mut read = self.store.sync_cursor(&space.id(), &device)?;
            if !batches.iter().any(|(seq, _)| *seq == read + 1) {
                // Newest first; one that does not check out (junk someone
                // left) gives way to the next.
                let mut candidates = snapshots.remove(&device).unwrap_or_default();
                candidates.sort_by_key(|c| std::cmp::Reverse(c.0));
                for (through, path) in candidates {
                    if through > read
                        && self.apply_snapshot(session, &device, through, &path, report)?
                    {
                        read = through;
                        break;
                    }
                }
            }
            let unread: Vec<(u64, String)> =
                batches.into_iter().filter(|(seq, _)| *seq > read).collect();
            for (seq, path) in unread {
                // Strictly in order: a batch missing for now (hidden, or not
                // pulled yet) stops this PC's batches until it is there.
                if seq != read + 1 {
                    break;
                }
                // In a team, one PC's bad batch stops that PC's batches
                // only: a member must not halt everyone's sync (and with it
                // an admin's means to remove them).
                let team = matches!(session.space, Space::Team(_));
                let opened = match self.open_signed(session, &device, &path) {
                    Ok(Some(body)) => serde_json::from_str::<Batch>(&body).ok(),
                    Ok(None) => break,
                    Err(_) if team => None,
                    Err(error) => return Err(error),
                };
                // A PC writes only its own versions.
                let Some(batch) = opened.filter(|batch| {
                    batch.device == device
                        && batch.seq == seq
                        && batch
                            .changes
                            .iter()
                            .all(|c| c.version.stamp.device == device)
                }) else {
                    if team {
                        report.refused += 1;
                        break;
                    }
                    return Err(corrupt(&path));
                };
                for change in batch.changes {
                    self.take_change(session, change, report, false)?;
                }
                self.read_to(session, &device, seq)?;
                read = seq;
            }
        }
        // A team's, all together, in the order they were made.
        let mut held = session.held.take();
        held.sort_by(|a, b| a.0.version.stamp.cmp(&b.0.version.stamp));
        for (change, catching_up) in &held {
            self.apply_change(session, change, report, *catching_up)?;
        }
        for (device, seq) in session.read_to.take() {
            self.store.set_sync_cursor(&space.id(), &device, seq)?;
        }
        Ok(())
    }

    /// Applies a version another PC wrote now (personal sync), or holds it
    /// to apply with the rest (a team).
    fn take_change(
        &self,
        session: &Session,
        change: Change,
        report: &mut SyncReport,
        catching_up: bool,
    ) -> AppResult<()> {
        match session.space {
            Space::Personal => self.apply_change(session, &change, report, catching_up),
            Space::Team(_) => {
                session.held.borrow_mut().push((change, catching_up));
                Ok(())
            }
        }
    }

    /// Records how far `device` was read: now (personal sync), or once what
    /// was held is applied (a team).
    fn read_to(&self, session: &Session, device: &DeviceId, seq: u64) -> AppResult<()> {
        match session.space {
            Space::Personal => Ok(self
                .store
                .set_sync_cursor(&session.space.id(), device, seq)?),
            Space::Team(_) => {
                session.read_to.borrow_mut().push((device.clone(), seq));
                Ok(())
            }
        }
    }

    /// A PC's batches through `through`, as one snapshot. Returns whether it
    /// was read. One that does not open or check out (not a member's, or
    /// junk someone with push access left) is skipped: it stops only that
    /// PC's batches, not the whole sync, as a missing batch would.
    fn apply_snapshot(
        &self,
        session: &Session,
        device: &DeviceId,
        through: u64,
        path: &str,
        report: &mut SyncReport,
    ) -> AppResult<bool> {
        let Ok(Some(body)) = self.open_signed(session, device, path) else {
            return Ok(false);
        };
        let Ok(snapshot) = serde_json::from_str::<Snapshot>(&body) else {
            return Ok(false);
        };
        // A PC writes only its own versions, in a file named for its seq.
        if snapshot.device != *device
            || snapshot.through_seq != through
            || snapshot
                .changes
                .iter()
                .any(|c| c.version.stamp.device != *device)
        {
            return Ok(false);
        }
        let mut former = session.former.borrow_mut();
        for f in snapshot.former {
            former.insert((f.kind, f.uid), f.bodies);
        }
        drop(former);
        for change in snapshot.changes {
            self.take_change(session, change, report, true)?;
        }
        self.read_to(session, device, through)?;
        Ok(true)
    }

    /// Once this PC has many batches in the repo, writes the newest version
    /// it made of each item as one snapshot and removes them (and older
    /// snapshots). A version replaced since by another PC's is left out:
    /// that one carries it in its history.
    ///
    /// Its folder only ever holds what this PC wrote: anything else there
    /// (left by someone with push access) goes.
    fn compact(&self, session: &Session) -> AppResult<()> {
        let space = &session.space;
        let own = layout::device_dir(&session.device);
        // Every batch it decided is written (nothing pending).
        if self.sync_setting(space, PENDING)?.is_some() {
            return Ok(());
        }
        let seq = self.sync_number(space, SEQ)?;
        let mut files = self.sync_remote.files(&session.dir, &own)?;
        // Its own snapshot is the one it remembers writing, not the
        // highest name there (someone with push access can add one).
        let newest_snapshot = self.sync_number(space, SNAPSHOT)?;
        let mine = |path: &str| {
            path == layout::device_info(&session.device)
                || path == crate::domain::team::layout::team_info(&session.device)
                || layout::batch_seq(path).is_some_and(|(_, s)| s <= seq)
                || layout::snapshot_seq(path)
                    .is_some_and(|(_, s)| newest_snapshot > 0 && s == newest_snapshot)
        };
        for path in files.iter().filter(|p| !mine(p)) {
            self.sync_remote.remove(&session.dir, path)?;
        }
        files.retain(|p| mine(p));
        let batches = files
            .iter()
            .filter(|p| layout::batch_seq(p).is_some())
            .count();
        if batches <= COMPACT_AFTER {
            return Ok(());
        }
        let through_seq = seq;
        let changes: Vec<Change> = self
            .store
            .sync_items(&space.id())?
            .into_iter()
            .filter(|c| c.version.stamp.device == session.device)
            .collect();
        let former = self.former_bodies(session, &files, &changes)?;
        let body = json(&Snapshot {
            device: session.device.clone(),
            through_seq,
            changes,
            former,
        });
        // Too big for one file once signed and sealed (it would be
        // refused): keep the batches.
        if body.len() > FILE_MAX / 8 {
            return Ok(());
        }
        let path = layout::snapshot(&session.device, through_seq);
        self.write_signed(session, &path, &body)?;
        self.set_sync_setting(space, SNAPSHOT, &through_seq.to_string())?;
        for old in files {
            let replaced = layout::batch_seq(&old).is_some_and(|(_, seq)| seq <= through_seq)
                || layout::snapshot_seq(&old).is_some_and(|(_, seq)| seq < through_seq);
            if replaced {
                self.sync_remote.remove(&session.dir, &old)?;
            }
        }
        Ok(())
    }

    /// For each item a snapshot keeps, the digests of what it held before:
    /// read from this PC's batches and snapshot it replaces.
    fn former_bodies(
        &self,
        session: &Session,
        files: &[String],
        changes: &[Change],
    ) -> AppResult<Vec<Former>> {
        let mut seen: HashMap<(ItemKind, String), Vec<String>> = HashMap::new();
        let mut note = |kind: ItemKind, uid: &str, digest: String| {
            let list = seen.entry((kind, uid.to_owned())).or_default();
            list.retain(|d| *d != digest);
            list.push(digest);
        };
        let mut ordered: Vec<&String> = files.iter().collect();
        // Oldest first: the snapshot (it comes before its batches), then
        // the batches by seq.
        ordered.sort_by_key(|p| match (layout::snapshot_seq(p), layout::batch_seq(p)) {
            (Some((_, seq)), _) => (0, seq),
            (_, Some((_, seq))) => (1, seq),
            _ => (2, 0),
        });
        for path in ordered {
            let Ok(Some(body)) = self.open_signed(session, &session.device, path) else {
                continue;
            };
            if layout::snapshot_seq(path).is_some() {
                let Ok(old) = serde_json::from_str::<Snapshot>(&body) else {
                    continue;
                };
                for f in old.former {
                    for digest in f.bodies {
                        note(f.kind, &f.uid, digest);
                    }
                }
                for c in old.changes {
                    if let Some(body) = &c.version.body {
                        note(c.kind, &c.uid, self.sync_crypto.digest(body.as_bytes()));
                    }
                }
            } else if let Ok(batch) = serde_json::from_str::<Batch>(&body) {
                for c in batch.changes {
                    if let Some(body) = &c.version.body {
                        note(c.kind, &c.uid, self.sync_crypto.digest(body.as_bytes()));
                    }
                }
            }
        }
        Ok(changes
            .iter()
            .filter_map(|c| {
                let mut bodies = seen.remove(&(c.kind, c.uid.clone()))?;
                if let Some(body) = &c.version.body {
                    let now = self.sync_crypto.digest(body.as_bytes());
                    bodies.retain(|d| *d != now);
                }
                let keep = bodies.len().saturating_sub(FORMER_MAX);
                bodies.drain(..keep);
                (!bodies.is_empty()).then(|| Former {
                    kind: c.kind,
                    uid: c.uid.clone(),
                    bodies,
                })
            })
            .collect())
    }

    fn apply_change(
        &self,
        session: &Session,
        change: &Change,
        report: &mut SyncReport,
        catching_up: bool,
    ) -> AppResult<()> {
        let space = &session.space;
        self.observe_stamp(space, &change.version.stamp)?;
        let known = self
            .store
            .sync_item(&space.id(), change.kind, &change.uid)?;
        // Checked here, by every PC that reads it.
        let allowed = match space {
            Space::Personal => change.kind != ItemKind::Suggestion,
            Space::Team(_) => self.team_allows(session, change, known.as_ref().map(|(_, v)| v))?,
        };
        if !allowed {
            report.refused += 1;
            return Ok(());
        }
        match merge(known.as_ref().map(|(_, v)| v), &change.version) {
            Merge::Ignore => {}
            Merge::Apply => {
                self.store.put_sync_item(&space.id(), change)?;
                self.bring_here(session, change)?;
                // Built on the version a conflict kept: settled elsewhere.
                // Not when it takes over a removed PC's version: nobody
                // chose then.
                for conflict in self.store.sync_conflicts(&space.id())? {
                    if !change.version.restates
                        && conflict.kind == change.kind
                        && conflict.uid == change.uid
                    {
                        self.store.delete_sync_conflict(conflict.id)?;
                    }
                }
                report.received += 1;
            }
            Merge::Conflict { incoming_wins } => {
                let (_, local) = known.expect("a conflict has a local version");
                let local_device = local.stamp.device.clone();
                let (kept, other) = if incoming_wins {
                    self.store.put_sync_item(&space.id(), change)?;
                    self.bring_here(session, change)?;
                    report.received += 1;
                    (change.version.clone(), local)
                } else {
                    (local, change.version.clone())
                };
                // Catching up through a snapshot, a version far behind
                // reads as made apart (its history only goes back so far):
                // the newer one is taken, unless the other was made here.
                let far_behind = catching_up && local_device != session.device;
                if change.kind.asks_on_conflict() && other.body != kept.body && !far_behind {
                    report.conflicts +=
                        self.record_conflict(space, change.kind, &change.uid, &kept, &other)?;
                }
            }
        }
        Ok(())
    }

    /// Writes a version into this PC's rows: personal sync's only (a team's
    /// notes live in its replica).
    fn bring_here(&self, session: &Session, change: &Change) -> AppResult<()> {
        if session.space == Space::Personal {
            self.materialize(session, change)?;
        }
        Ok(())
    }

    /// Brings in synced items this PC never had: a project that became
    /// personal here, or one added later.
    fn bring_waiting(&self, session: &Session) -> AppResult<()> {
        let space = &session.space;
        for change in self.store.sync_items(&space.id())? {
            if change.version.is_deletion()
                || !Self::scope_here(session, &change.scope)
                || self.store.sync_link(change.kind, &change.uid)?.is_some()
            {
                continue;
            }
            self.materialize(session, &change)?;
        }
        Ok(())
    }

    /// Indexes the rows here not synced yet, by what they hold.
    fn index_twins(&self, session: &Session) -> AppResult<()> {
        let mut twins = session.twins.borrow_mut();
        let mut digests = session.twin_digests.borrow_mut();
        for item in self.local_items(session)? {
            if self
                .store
                .sync_link_of(item.kind, &item.local_id)?
                .is_none()
            {
                let digest = self.sync_crypto.digest(item.body.as_bytes());
                let key = (item.kind, item.scope, item.body);
                digests.insert(digest, key.clone());
                twins.entry(key).or_default().push(item.local_id);
            }
        }
        Ok(())
    }

    /// A row here equal to `change` and not synced yet, taken from the index.
    fn take_twin(session: &Session, change: &Change) -> Option<String> {
        let body = change.version.body.clone()?;
        session
            .twins
            .borrow_mut()
            .get_mut(&(change.kind, change.scope.clone(), body))?
            .pop()
    }

    /// A row here not synced yet that holds what `change`'s item held
    /// before (a snapshot's digests), taken from the index.
    fn take_former_twin(&self, session: &Session, change: &Change) -> Option<String> {
        let former = session.former.borrow();
        let keys = session.twin_digests.borrow();
        let mut twins = session.twins.borrow_mut();
        former
            .get(&(change.kind, change.uid.clone()))?
            .iter()
            .filter_map(|digest| keys.get(digest))
            .filter(|(kind, scope, _)| *kind == change.kind && *scope == change.scope)
            .find_map(|key| twins.get_mut(key)?.pop())
    }

    /// The project a linked row is in now; `None` when the row is gone.
    fn row_project(&self, kind: ItemKind, local_id: &str) -> AppResult<Option<Option<PathBuf>>> {
        let row = local_id.parse::<i64>().ok();
        Ok(match (kind, row) {
            (ItemKind::Note, Some(id)) => self.store.note(id)?.map(|n| n.project),
            (ItemKind::Skill, Some(id)) => self.store.skill(id)?.map(|s| s.project),
            (ItemKind::Template, Some(id)) => self
                .store
                .template_records()?
                .into_iter()
                .find(|r| r.id == id)
                .map(|r| r.project),
            _ => None,
        })
    }

    /// Writes a synced version into this PC's notes, skills, templates or
    /// themes, and links the row to it. Nothing is written for a project not
    /// synced here (or two projects here with its remote and no row yet):
    /// the version waits in the replica. Returns whether it was written.
    fn materialize(&self, session: &Session, change: &Change) -> AppResult<bool> {
        if !Self::scope_here(session, &change.scope) {
            return Ok(false);
        }
        let mut link: Option<SyncLink> = self.store.sync_link(change.kind, &change.uid)?;
        let made_now = |link: &SyncLink| {
            session
                .made
                .borrow()
                .contains(&(change.kind, link.local_id.clone()))
        };
        if link.as_ref().is_none_or(made_now)
            && let Some(local_id) = Self::take_twin(session, change)
        {
            // The same item, already here: adopt the row (and drop the one
            // this first sync made for an older version of it).
            if let Some(link) = &link {
                self.delete_row(change.kind, &link.local_id)?;
            }
            self.store
                .link_sync_item(change.kind, &change.uid, &local_id, &change.version)?;
            return Ok(true);
        }
        if link.is_none()
            && let Some(local_id) = self.take_former_twin(session, change)
        {
            // A row equal to an older version of it (a PC joining again
            // after the others compacted): adopt it, and bring it up to
            // date below.
            self.store
                .link_sync_item(change.kind, &change.uid, &local_id, &change.version)?;
            link = self.store.sync_link(change.kind, &change.uid)?;
        }
        let row_project = match &link {
            Some(link) => self.row_project(change.kind, &link.local_id)?,
            None => None,
        };
        let project = match (&change.scope, row_project) {
            (Scope::Global, _) => None,
            // A row stays in the project it is in.
            (Scope::Project(_), Some(project)) => project,
            (Scope::Project(id), None) => match session.projects.get(id).map(Vec::as_slice) {
                Some([only]) => Some(only.clone()),
                _ => return Ok(false),
            },
        };
        let row = link.as_ref().and_then(|l| l.local_id.parse::<i64>().ok());
        let Some(body) = &change.version.body else {
            if let Some(link) = &link {
                self.delete_row(change.kind, &link.local_id)?;
                self.store.unlink_sync_item(change.kind, &change.uid)?;
            }
            return Ok(true);
        };
        // A body this version cannot read (a newer Hedy's) waits in the
        // replica instead of stopping the sync.
        let local_id = match change.kind {
            ItemKind::Note => {
                let Ok(body) = serde_json::from_str::<NoteBody>(body) else {
                    return Ok(false);
                };
                self.store
                    .write_synced_note(row, project.as_deref(), &body)?
                    .to_string()
            }
            ItemKind::Skill => {
                let Ok(body) = serde_json::from_str::<SkillBody>(body) else {
                    return Ok(false);
                };
                self.store
                    .write_synced_skill(row, project.as_deref(), &body)?
                    .to_string()
            }
            ItemKind::Template => {
                let Ok(body) = serde_json::from_str::<TemplateBody>(body) else {
                    return Ok(false);
                };
                // A built-in edited here too, not synced yet (a first sync):
                // its row is not overwritten; it is sent as this PC's edit,
                // on top of this version.
                if row.is_none()
                    && let Some(builtin) = &body.builtin
                    && self
                        .store
                        .template_records()?
                        .iter()
                        .any(|r| r.builtin.as_ref() == Some(builtin) && r.project == project)
                {
                    return Ok(false);
                }
                self.store
                    .save_template(&TemplateRecord {
                        id: row.unwrap_or(0),
                        project,
                        name: body.name,
                        text: body.text,
                        builtin: body.builtin,
                        deleted: body.deleted,
                    })?
                    .to_string()
            }
            ItemKind::Theme => {
                let Ok(body) = serde_json::from_str::<ThemeBody>(body) else {
                    return Ok(false);
                };
                // Each PC picks the id: two themes made apart with one name
                // are two themes.
                let id = match &link {
                    Some(link) => link.local_id.clone(),
                    None => new_theme_id(&body.name, |id| !matches!(self.themes.get(id), Ok(None))),
                };
                // One that does not validate here (a newer Hedy's) waits.
                let Ok(theme) =
                    CustomTheme::new(id, &body.name, body.base, body.colors, body.origin)
                else {
                    return Ok(false);
                };
                self.themes.save(&theme)?;
                theme.id
            }
            // A team's only: never a row here.
            ItemKind::Suggestion => return Ok(false),
        };
        if link.is_none() && !session.twins.borrow().is_empty() {
            session
                .made
                .borrow_mut()
                .insert((change.kind, local_id.clone()));
        }
        self.store
            .link_sync_item(change.kind, &change.uid, &local_id, &change.version)?;
        Ok(true)
    }

    fn delete_row(&self, kind: ItemKind, local_id: &str) -> AppResult<()> {
        let row = local_id.parse::<i64>().unwrap_or(-1);
        match kind {
            ItemKind::Note => self.store.delete_note(row)?,
            ItemKind::Skill => self.store.delete_skill(row)?,
            ItemKind::Template => self.store.delete_template(row)?,
            ItemKind::Theme => self.themes.delete(local_id)?,
            ItemKind::Suggestion => {}
        }
        Ok(())
    }

    /// The sync's state for the status bar and Ajustes; `None` when it is
    /// not set up. Reads the clone only (no network): the trusted epoch's
    /// PCs, and their names as each signed them.
    pub fn sync_status(&self) -> AppResult<Option<SyncStatus>> {
        Ok(self.space_status(PERSONAL)?.map(|(status, _)| status))
    }

    /// A space's state, from the clone only (no network); `None` when it is
    /// not set up. The trusted epoch's PCs, with their names as each signed
    /// them and, in a team, their seats.
    pub(super) fn space_status(
        &self,
        space: &Space,
    ) -> AppResult<Option<(SyncStatus, Vec<SpaceMember>)>> {
        let Some(url) = self.sync_setting(space, URL)? else {
            return Ok(None);
        };
        let device = DeviceId(self.sync_setting(space, DEVICE)?.unwrap_or_default());
        let dir = self.sync_dir(space)?;
        let keys = self.keys(space)?;
        let trusted = self.sync_number(space, EPOCH)? as u32;
        let pins = self.pins(space)?;
        let mut members = Vec::new();
        let trusted_file = self
            .epochs(&dir)?
            .into_iter()
            .find(|(n, f)| {
                *n == trusted && pins.get(n) == Some(&self.sync_crypto.digest(f.as_bytes()))
            })
            .map(|(_, f)| f);
        if let Some(file) = trusted_file {
            let in_epoch = self.sync_crypto.epoch_members(&file)?.members;
            let session = Session {
                space: space.clone(),
                dir: dir.clone(),
                device: device.clone(),
                me: keys.device.clone(),
                current: String::new(),
                projects: HashMap::new(),
                project_ids: HashMap::new(),
                members: in_epoch
                    .iter()
                    .map(|m| (m.id.clone(), m.public.clone()))
                    .collect(),
                twins: RefCell::default(),
                made: RefCell::default(),
                former: RefCell::default(),
                twin_digests: RefCell::default(),
                seats: HashMap::new(),
                held: RefCell::default(),
                read_to: RefCell::default(),
                keys: keys.epochs.values().cloned().collect(),
            };
            for member in in_epoch.into_iter().filter(|m| m.id != RECOVERY_MEMBER) {
                let id = DeviceId(member.id);
                let info: Option<DeviceInfo> = self
                    .open_signed(&session, &id, &layout::device_info(&id))
                    .ok()
                    .flatten()
                    .and_then(|body| serde_json::from_str(&body).ok());
                members.push(SpaceMember {
                    seat: member.seat,
                    person_name: info.as_ref().and_then(|i| i.person_name.clone()),
                    device: SyncDevice {
                        this_pc: id == device,
                        name: info.as_ref().map(|i| i.name.clone()).unwrap_or_default(),
                        last_sync: info.map(|i| i.last_sync),
                        id,
                    },
                });
            }
        }
        members.sort_by_key(|m| !m.device.this_pc);
        let status = SyncStatus {
            url,
            device,
            last_sync: self
                .sync_setting(space, LAST_SYNC)?
                .and_then(|v| v.parse().ok()),
            last_error: self.sync_setting(space, LAST_ERROR)?,
            devices: members.iter().map(|m| m.device.clone()).collect(),
            conflicts: self.store.sync_conflicts(&space.id())?.len(),
        };
        Ok(Some((status, members)))
    }

    /// This PC stops syncing: its keys, the clone, the sync settings and
    /// state go. Its notes, skills, templates and themes stay, and so does
    /// the repo (the other PCs keep syncing). Joining again starts fresh.
    /// It also clears what a setup or join cut short left behind, and says
    /// whether the sync was set up. This PC stays a member of the key epoch
    /// until it is removed from another PC.
    pub fn sync_leave(&self) -> AppResult<bool> {
        let space = PERSONAL;
        let _guard = self.sync_guard(space)?;
        let was_set_up = self.space_is_set_up(space)?;
        self.sync_remote.discard_local(&self.sync_dir(space)?)?;
        self.secrets.delete_sync_keys(&space.id())?;
        self.store.clear_sync_state(&space.id())?;
        // URL first: what says the sync is set up.
        for key in [
            URL, DEVICE, NAME, RECOVERY, EPOCH, PINS, SEQ, PENDING, OUTBOX, CLOCK, LAST_SYNC,
            LAST_ERROR, INFO_AT, SNAPSHOT,
        ] {
            self.clear_sync_setting(space, key)?;
        }
        Ok(was_set_up)
    }

    /// Removes another PC (by its id or its name): a new key epoch leaves it
    /// out, so it cannot read anything written from now on, and its files
    /// go. This PC syncs first, then takes over what that PC wrote last (a
    /// new version of each item, same content), so the other PCs get it
    /// even if they had not read it yet. Returns the sync that sends it.
    pub fn sync_remove_device(&self, who: &str) -> AppResult<SyncReport> {
        let space = PERSONAL;
        let _guard = self.sync_guard(space)?;
        let me = DeviceId(
            self.sync_setting(space, DEVICE)?
                .ok_or(AppError::SyncNotSetUp)?,
        );
        // Everything it sent so far, first; the epoch this PC checked last
        // is then the newest.
        self.sync_locked(space)?;
        let who = who.trim();
        let devices = self.sync_status()?.ok_or(AppError::SyncNotSetUp)?.devices;
        let named: Vec<&SyncDevice> = devices.iter().filter(|d| d.name.trim() == who).collect();
        let device = match devices.iter().find(|d| d.id.as_str() == who) {
            Some(device) => device.clone(),
            None if named.len() > 1 => return Err(AppError::SyncDeviceAmbiguous(who.to_owned())),
            None => (*named
                .first()
                .ok_or_else(|| AppError::SyncDeviceNotFound(who.to_owned()))?)
            .clone(),
        };
        if device.id == me {
            return Err(AppError::SyncRemoveThisPc);
        }
        if !device.id.is_valid() {
            return Err(AppError::SyncDeviceNotFound(who.to_owned()));
        }
        let dir = self.sync_dir(space)?;
        let keys = self.keys(space)?;
        let newest = self.sync_number(space, EPOCH)? as u32;
        let epochs = self.epochs(&dir)?;
        let file = epochs
            .iter()
            .find(|(n, _)| *n == newest)
            .map(|(_, f)| f.clone())
            .ok_or_else(|| corrupt(&layout::epoch(newest)))?;
        // Another PC changed the keys meanwhile (its epoch came in while
        // this sync pushed): never write over it; the next sync checks it.
        // One that does not follow is not trusted, and is written over.
        if let Some((_, next)) = epochs.iter().find(|(n, _)| *n == newest + 1)
            && self.sync_crypto.verify_next_epoch(&file, next).is_ok()
        {
            return Err(AppError::SyncBusy);
        }
        let EpochInfo { team, members, .. } = self.sync_crypto.epoch_members(&file)?;
        let remaining: Vec<EpochMember> = members
            .into_iter()
            .filter(|m| m.id != device.id.as_str())
            .collect();
        let (next, _) =
            self.sync_crypto
                .new_epoch(newest + 1, team, &remaining, me.as_str(), &keys.device)?;
        self.sync_remote
            .write(&dir, &layout::epoch(newest + 1), next.as_bytes())?;
        // Its batches and snapshots go; its `device.hedy` stays, so a
        // commit it has not pushed yet (which changes it) still rebases and
        // it then learns it was removed.
        for path in self
            .sync_remote
            .files(&dir, &layout::device_dir(&device.id))?
        {
            if path != layout::device_info(&device.id) {
                self.sync_remote.remove(&dir, &path)?;
            }
        }
        // Another PC wrote this epoch first: start from the repo again.
        if let Err(error) = self.sync_remote.push(&dir, "hedy sync") {
            let _ = self.sync_remote.discard_local(&dir);
            return Err(error.into());
        }
        // Each version keeps its time (a later edit elsewhere still wins);
        // only the PC that answers for it changes.
        let taken: Vec<(Stamp, Change)> = self
            .store
            .sync_items(&space.id())?
            .into_iter()
            .filter(|item| item.version.stamp.device == device.id)
            .map(|item| {
                let old = item.version.stamp.clone();
                let stamp = Stamp {
                    device: me.clone(),
                    ..old.clone()
                };
                let version = item.version.restated(stamp);
                (old, Change { version, ..item })
            })
            .collect();
        // Kept to send first, so a failure below loses none of them.
        let mut outbox: Vec<Change> = self.sync_json(space, OUTBOX)?.unwrap_or_default();
        outbox.extend(taken.iter().map(|(_, change)| change.clone()));
        self.set_sync_setting(space, OUTBOX, &json(&outbox))?;
        for (old, change) in &taken {
            self.store.put_sync_item(&space.id(), change)?;
            // A row that holds that version: only its base moves. One that
            // waits (or was edited here) keeps its base.
            if let Some(link) = self.store.sync_link(change.kind, &change.uid)?
                && link.base_stamp == *old
            {
                self.store.link_sync_item(
                    change.kind,
                    &change.uid,
                    &link.local_id,
                    &change.version,
                )?;
            }
        }
        // The next sync checks the new epoch (signed here) and sends them.
        let report = self.sync_locked(space)?;
        self.clear_sync_setting(space, LAST_ERROR)?;
        Ok(report)
    }

    pub fn sync_conflicts(&self) -> AppResult<Vec<SyncConflict>> {
        Ok(self.store.sync_conflicts(&PERSONAL.id())?)
    }

    /// Settles a conflict with a new version on top of the one in use: the
    /// other one (`use_other`) or the same again. It is written here and
    /// sent with the next sync, which also clears the conflict on the other
    /// PCs.
    pub fn resolve_sync_conflict(&self, id: i64, use_other: bool) -> AppResult<()> {
        let space = PERSONAL;
        let _guard = self.sync_guard(space)?;
        let conflict = self
            .store
            .sync_conflicts(&space.id())?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or(AppError::SyncConflictNotFound(id))?;
        let (scope, known) = self
            .store
            .sync_item(&space.id(), conflict.kind, &conflict.uid)?
            .ok_or(AppError::SyncConflictNotFound(id))?;
        let device = DeviceId(
            self.sync_setting(space, DEVICE)?
                .ok_or(AppError::SyncNotSetUp)?,
        );
        let body = if use_other {
            conflict.other.body.clone()
        } else {
            known.body.clone()
        };
        let change = Change {
            kind: conflict.kind,
            uid: conflict.uid.clone(),
            scope,
            version: known.replaced(self.next_stamp(space, &device)?, body),
        };
        self.store.put_sync_item(&space.id(), &change)?;
        let keys = self.keys(space)?;
        let name_key = keys.epochs.values().next().cloned().unwrap_or_default();
        let SyncedProjects {
            projects,
            project_ids,
        } = self.synced_projects(space, &name_key)?;
        let session = Session {
            space: space.clone(),
            dir: self.sync_dir(space)?,
            device,
            me: keys.device.clone(),
            current: String::new(),
            projects,
            project_ids,
            members: HashMap::new(),
            twins: RefCell::default(),
            made: RefCell::default(),
            former: RefCell::default(),
            twin_digests: RefCell::default(),
            seats: HashMap::new(),
            held: RefCell::default(),
            read_to: RefCell::default(),
            keys: Vec::new(),
        };
        self.materialize(&session, &change)?;
        let mut outbox: Vec<Change> = self.sync_json(space, OUTBOX)?.unwrap_or_default();
        outbox.push(change);
        self.set_sync_setting(space, OUTBOX, &json(&outbox))?;
        Ok(self.store.delete_sync_conflict(id)?)
    }
}

/// When an item was created, for kinds whose rows can be reused (notes and
/// skills): what tells a new item in an old row from an edit.
fn birth(kind: ItemKind, body: &str) -> Option<i64> {
    match kind {
        ItemKind::Note => serde_json::from_str::<NoteBody>(body)
            .ok()
            .map(|b| b.created_at),
        ItemKind::Skill => serde_json::from_str::<SkillBody>(body)
            .ok()
            .map(|b| b.created_at),
        ItemKind::Template | ItemKind::Theme | ItemKind::Suggestion => None,
    }
}

/// The repo's only plain file besides the key epochs.
fn readme() -> String {
    "# Hedy\n\nPersonal sync of Hedy (notes, skills, templates and themes), sealed with \
     XChaCha20-Poly1305 and signed on each PC before it is pushed. Nothing here can be read \
     without the owner's keys.\n"
        .into()
}

mod team;

pub use team::{
    TeamInvite, TeamJoin, TeamNote, TeamPerson, TeamStatus, TeamSuggestion, TeamSummary,
    TeamWriteOutcome,
};

#[cfg(test)]
mod tests;
