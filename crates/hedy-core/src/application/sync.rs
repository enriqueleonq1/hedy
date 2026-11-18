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

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::MutexGuard;

use serde::{Deserialize, Serialize};

use super::ports::{SyncConflict, SyncLink, TemplateRecord};
use super::{AppError, AppResult, Hedy};
use crate::domain::sync::{
    Batch, Change, DeviceId, ItemKind, Merge, NoteBody, RECOVERY_MEMBER, RepoIdentity, Scope,
    SkillBody, Stamp, TemplateBody, ThemeBody, Version, layout, merge,
};
use crate::domain::theme::{CustomTheme, new_theme_id};
use crate::tr;

/// Settings of this PC's sync (plain, not secret).
const URL: &str = "sync.url";
const DEVICE: &str = "sync.device";
const NAME: &str = "sync.device_name";
/// The recovery key's public keys: the root every epoch is checked from.
const RECOVERY: &str = "sync.recovery";
/// The newest epoch checked and trusted.
const EPOCH: &str = "sync.epoch";
/// SHA-256 of every epoch file checked (JSON map): a checked epoch never
/// changes.
const PINS: &str = "sync.pins";
/// This PC's last batch.
const SEQ: &str = "sync.seq";
/// A batch decided but not yet written to the clone (JSON [`Pending`]).
const PENDING: &str = "sync.pending";
/// Versions made outside a sync (a settled conflict), sent with the next.
const OUTBOX: &str = "sync.outbox";
/// The newest stamp made or seen (JSON), so new ones are always later.
const CLOCK: &str = "sync.clock";
const LAST_SYNC: &str = "sync.last_sync";
/// When this PC last wrote its name and last sync to the repo.
const INFO_AT: &str = "sync.info_at";

/// This PC writes its name and last sync at most this often (seconds), so
/// a sync with nothing new does not make a commit.
const INFO_EVERY: i64 = 3600;
/// A stamp further ahead than this (ms) does not move the clock: a PC with
/// a wrong date must not pin every PC's clock in the future.
const CLOCK_AHEAD_MAX: i64 = 24 * 3600 * 1000;
/// Files in the repo bigger than this are refused, not read.
const FILE_MAX: usize = 16 * 1024 * 1024;

const KINDS: [ItemKind; 4] = [
    ItemKind::Note,
    ItemKind::Skill,
    ItemKind::Template,
    ItemKind::Theme,
];

/// What the keyring keeps: this PC's keys and every epoch key it has.
#[derive(Serialize, Deserialize)]
struct Keys {
    device: String,
    epochs: BTreeMap<u32, String>,
}

/// A PC's name and last sync, as it writes them to the repo.
#[derive(Serialize, Deserialize)]
struct DeviceInfo {
    name: String,
    last_sync: i64,
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
    /// The PCs of the trusted key epoch, this one first.
    pub devices: Vec<SyncDevice>,
    pub conflicts: usize,
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
}

/// The projects that sync here, both ways round (see [`Session`]).
struct SyncedProjects {
    projects: HashMap<String, Vec<PathBuf>>,
    project_ids: HashMap<PathBuf, String>,
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
    /// One sync operation at a time; a panic in another does not block the
    /// next.
    fn sync_guard(&self) -> MutexGuard<'_, ()> {
        self.sync_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn sync_setting(&self, key: &str) -> AppResult<Option<String>> {
        Ok(self.store.get_setting(key)?)
    }

    fn set_sync_setting(&self, key: &str, value: &str) -> AppResult<()> {
        Ok(self.store.set_setting(key, Some(value))?)
    }

    fn sync_json<T: serde::de::DeserializeOwned>(&self, key: &str) -> AppResult<Option<T>> {
        Ok(self
            .sync_setting(key)?
            .and_then(|text| serde_json::from_str(&text).ok()))
    }

    fn sync_number(&self, key: &str) -> AppResult<u64> {
        Ok(self
            .sync_setting(key)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0))
    }

    /// The sync repo's clone, inside Hedy's data folder (never a relative
    /// path: a failed setup deletes it).
    fn sync_dir(&self) -> AppResult<PathBuf> {
        match self.store.data_dir() {
            Some(dir) if dir.is_absolute() => Ok(dir.join("sync").join("repo")),
            _ => Err(corrupt("data dir")),
        }
    }

    pub fn sync_is_set_up(&self) -> AppResult<bool> {
        Ok(self.sync_setting(URL)?.is_some())
    }

    fn keys(&self) -> AppResult<Keys> {
        let text = self.secrets.sync_keys()?.ok_or(AppError::SyncNotSetUp)?;
        serde_json::from_str(&text).map_err(|_| corrupt("keyring"))
    }

    fn save_keys(&self, keys: &Keys) -> AppResult<()> {
        Ok(self.secrets.set_sync_keys(&json(keys))?)
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
                Ok((inside, _)) if inside == number => epochs.push((number, text)),
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
        let _guard = self.sync_guard();
        if self.sync_is_set_up()? {
            return Err(AppError::SyncAlreadySetUp);
        }
        let dir = self.sync_dir()?;
        self.sync_remote.open(url, &dir)?;
        if !self.epochs(&dir)?.is_empty() {
            return Err(AppError::SyncRepoInUse);
        }
        let device = DeviceId(self.sync_crypto.random_hex(8)?);
        let me = self.sync_crypto.new_member()?;
        let (words, recovery) = self.sync_crypto.new_recovery()?;
        let recovery_public = self.sync_crypto.member_public(&recovery)?;
        let members = [
            (RECOVERY_MEMBER.to_owned(), recovery_public.clone()),
            (device.0.clone(), self.sync_crypto.member_public(&me)?),
        ];
        let (file, key) = self
            .sync_crypto
            .new_epoch(1, &members, RECOVERY_MEMBER, &recovery)?;
        self.sync_remote
            .write(&dir, &layout::epoch(1), file.as_bytes())?;
        self.sync_remote
            .write(&dir, layout::README, readme().as_bytes())?;
        let keys = Keys {
            device: me,
            epochs: BTreeMap::from([(1, key)]),
        };
        self.publish_epoch(&dir, &keys)?;
        let pins = BTreeMap::from([(1, self.sync_crypto.digest(file.as_bytes()))]);
        self.remember_sync(url, &device, device_name, &recovery_public, 1, &pins)?;
        // The sync is set up: the words must reach the user even when this
        // first sync fails (the next one sends what is left).
        let _ = self.sync_locked();
        Ok(words)
    }

    /// Joins the sync at `url` with the recovery key's `words`: checks every
    /// epoch from the recovery key, adds this PC in a new epoch, then syncs
    /// (which brings everything in).
    pub fn sync_join(&self, url: &str, words: &str, device_name: &str) -> AppResult<SyncReport> {
        let _guard = self.sync_guard();
        if self.sync_is_set_up()? {
            return Err(AppError::SyncAlreadySetUp);
        }
        let dir = self.sync_dir()?;
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
        for (i, (number, file)) in epochs.iter().enumerate() {
            if i > 0 {
                self.sync_crypto.verify_next_epoch(&epochs[i - 1].1, file)?;
            }
            epoch_keys.insert(
                *number,
                self.sync_crypto
                    .open_epoch(file, RECOVERY_MEMBER, &recovery)?,
            );
            pins.insert(*number, self.sync_crypto.digest(file.as_bytes()));
        }
        let (last, last_file) = epochs.last().expect("not empty");
        let device = DeviceId(self.sync_crypto.random_hex(8)?);
        let me = self.sync_crypto.new_member()?;
        let (_, mut members) = self.sync_crypto.epoch_members(last_file)?;
        members.push((device.0.clone(), self.sync_crypto.member_public(&me)?));
        let next = last + 1;
        let (file, key) = self
            .sync_crypto
            .new_epoch(next, &members, RECOVERY_MEMBER, &recovery)?;
        self.sync_remote
            .write(&dir, &layout::epoch(next), file.as_bytes())?;
        epoch_keys.insert(next, key);
        pins.insert(next, self.sync_crypto.digest(file.as_bytes()));
        let keys = Keys {
            device: me,
            epochs: epoch_keys,
        };
        self.publish_epoch(&dir, &keys)?;
        self.remember_sync(url, &device, device_name, &recovery_public, next, &pins)?;
        self.sync_locked()
    }

    /// Keeps this PC's keys and pushes the new epoch; when the repo refuses
    /// it (another PC wrote the same epoch first), forgets both, so a retry
    /// starts clean.
    fn publish_epoch(&self, dir: &Path, keys: &Keys) -> AppResult<()> {
        self.save_keys(keys)?;
        if let Err(error) = self.sync_remote.push(dir, "hedy sync") {
            let _ = self.secrets.delete_sync_keys();
            let _ = self.sync_remote.discard_local(dir);
            return Err(error.into());
        }
        Ok(())
    }

    fn remember_sync(
        &self,
        url: &str,
        device: &DeviceId,
        name: &str,
        recovery_public: &str,
        epoch: u32,
        pins: &BTreeMap<u32, String>,
    ) -> AppResult<()> {
        self.set_sync_setting(DEVICE, device.as_str())?;
        self.set_sync_setting(NAME, name.trim())?;
        self.set_sync_setting(RECOVERY, recovery_public)?;
        self.set_sync_setting(EPOCH, &epoch.to_string())?;
        self.set_sync_setting(PINS, &json(pins))?;
        self.set_sync_setting(SEQ, "0")?;
        // Last: what says the sync is set up.
        self.set_sync_setting(URL, url)
    }

    /// One sync, now.
    pub fn sync_now(&self) -> AppResult<SyncReport> {
        let _guard = self.sync_guard();
        self.sync_locked()
    }

    fn sync_locked(&self) -> AppResult<SyncReport> {
        let url = self.sync_setting(URL)?.ok_or(AppError::SyncNotSetUp)?;
        let device = DeviceId(self.sync_setting(DEVICE)?.ok_or(AppError::SyncNotSetUp)?);
        let dir = self.sync_dir()?;
        self.sync_remote.open(&url, &dir)?;
        let session = self.sync_session(dir, device)?;
        let mut report = SyncReport::default();

        self.write_pending(&session)?;
        // What changed here first, so the other PCs' versions meet it.
        let mut outgoing: Vec<Outgoing> = self
            .sync_json::<Vec<Change>>(OUTBOX)?
            .unwrap_or_default()
            .into_iter()
            .map(|change| Outgoing {
                change,
                local_id: None,
            })
            .collect();
        outgoing.extend(self.local_changes(&session, &mut report)?);
        report.sent = outgoing.len();
        if !outgoing.is_empty() {
            let seq = self.sync_number(SEQ)? + 1;
            self.set_sync_setting(PENDING, &json(&Pending { seq, outgoing }))?;
            self.set_sync_setting(SEQ, &seq.to_string())?;
            self.store.set_setting(OUTBOX, None)?;
            self.write_pending(&session)?;
        }
        // This PC's batch leaves even when another PC's cannot be applied.
        let applied = self
            .apply_batches(&session, &mut report)
            .and_then(|()| self.bring_waiting(&session));

        let now = self.now_secs();
        let info_at = self.sync_number(INFO_AT)? as i64;
        if report.sent > 0 || now - info_at >= INFO_EVERY {
            let info = DeviceInfo {
                name: self.sync_setting(NAME)?.unwrap_or_default(),
                last_sync: now,
            };
            self.write_signed(
                &session,
                &layout::device_info(&session.device),
                &json(&info),
            )?;
            self.set_sync_setting(INFO_AT, &now.to_string())?;
        }
        self.sync_remote.push(&session.dir, "hedy sync")?;
        applied?;
        self.set_sync_setting(LAST_SYNC, &now.to_string())?;
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
        let Some(pending) = self.sync_json::<Pending>(PENDING)? else {
            return Ok(());
        };
        // Saved before SEQ moved (a crash in between): never reuse its seq.
        if self.sync_number(SEQ)? < pending.seq {
            self.set_sync_setting(SEQ, &pending.seq.to_string())?;
        }
        for out in &pending.outgoing {
            let change = &out.change;
            self.store.put_sync_item(change)?;
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
        Ok(self.store.set_setting(PENDING, None)?)
    }

    /// Checks the key epochs: every one already checked must be unchanged
    /// (its pinned hash), and any newer one signed by a member of the one
    /// before. Keeps the new keys; fails when the newest leaves this PC out.
    fn sync_session(&self, dir: PathBuf, device: DeviceId) -> AppResult<Session> {
        let mut keys = self.keys()?;
        let trusted = self.sync_number(EPOCH)? as u32;
        let mut pins = self.pins()?;
        let recovery = self.sync_setting(RECOVERY)?.unwrap_or_default();
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
            self.sync_crypto.verify_next_epoch(&previous, file)?;
            // The recovery key stays a member: it is how a PC joins.
            let (_, members) = self.sync_crypto.epoch_members(file)?;
            if !members
                .iter()
                .any(|(id, public)| id == RECOVERY_MEMBER && *public == recovery)
            {
                return Err(corrupt(&layout::epoch(*number)));
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
        if newest != trusted {
            self.save_keys(&keys)?;
            self.set_sync_setting(EPOCH, &newest.to_string())?;
        }
        self.set_sync_setting(PINS, &json(&pins))?;
        let current = keys
            .epochs
            .get(&newest)
            .cloned()
            .ok_or(AppError::SyncRemoved)?;
        let name_key = keys
            .epochs
            .values()
            .next()
            .cloned()
            .ok_or(AppError::SyncRemoved)?;
        let (_, members) = self.sync_crypto.epoch_members(&previous)?;
        let SyncedProjects {
            projects,
            project_ids,
        } = self.synced_projects(&name_key)?;
        Ok(Session {
            dir,
            device,
            me: keys.device.clone(),
            keys: keys.epochs.into_values().collect(),
            current,
            projects,
            project_ids,
            members: members.into_iter().collect(),
        })
    }

    /// The pinned epoch hashes. A setting that does not read fails closed:
    /// nothing would be checked otherwise.
    fn pins(&self) -> AppResult<BTreeMap<u32, String>> {
        match self.sync_setting(PINS)? {
            Some(text) => serde_json::from_str(&text).map_err(|_| corrupt(PINS)),
            None => Ok(BTreeMap::new()),
        }
    }

    /// The projects whose memory is personal here, by their sealed id (of
    /// `origin`'s `owner/name`, what matches them on other PCs). Two
    /// projects here with the same remote share one.
    fn synced_projects(&self, name_key: &str) -> AppResult<SyncedProjects> {
        let mut projects: HashMap<String, Vec<PathBuf>> = HashMap::new();
        let mut project_ids = HashMap::new();
        for project in self.store.list_projects()? {
            if !self
                .effective_memory_home(&project.path)?
                .is_some_and(|home| home.syncs())
            {
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
    fn next_stamp(&self, device: &DeviceId) -> AppResult<Stamp> {
        let last: Option<Stamp> = self.sync_json(CLOCK)?;
        let stamp = Stamp::next(last.as_ref(), self.clock.now_ms(), device);
        self.set_sync_setting(CLOCK, &json(&stamp))?;
        Ok(stamp)
    }

    /// Keeps the clock ahead of a stamp from another PC (not one far in the
    /// future: a PC with a wrong date).
    fn observe_stamp(&self, seen: &Stamp) -> AppResult<()> {
        if seen.ms > self.clock.now_ms() + CLOCK_AHEAD_MAX {
            return Ok(());
        }
        let last: Option<Stamp> = self.sync_json(CLOCK)?;
        if last.as_ref().is_none_or(|l| seen > l) {
            self.set_sync_setting(CLOCK, &json(seen))?;
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
        let mut out = Vec::new();
        for item in self.local_items(session)? {
            let link = self.store.sync_link_of(item.kind, &item.local_id)?;
            let known = match &link {
                Some(link) => self.store.sync_item(item.kind, &link.uid)?,
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
                    let stamp = self.next_stamp(&session.device)?;
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
            let stamp = self.next_stamp(&session.device)?;
            let version = known.replaced(stamp, Some(item.body.clone()));
            // Edited here while it changed (or was deleted) elsewhere.
            if changed_elsewhere
                && item.kind.asks_on_conflict()
                && known.body.as_deref() != Some(item.body.as_str())
            {
                report.conflicts += self.record_conflict(item.kind, &link.uid, &version, &known)?;
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
                let Some((scope, known)) = self.store.sync_item(kind, &link.uid)? else {
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
                let stamp = self.next_stamp(&session.device)?;
                let version = known.replaced(stamp, None);
                if known.stamp != link.base_stamp && kind.asks_on_conflict() {
                    report.conflicts += self.record_conflict(kind, &link.uid, &version, &known)?;
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
        kind: ItemKind,
        uid: &str,
        kept: &Version,
        other: &Version,
    ) -> AppResult<usize> {
        let known = self.store.sync_conflicts()?.into_iter().any(|c| {
            c.kind == kind
                && c.uid == uid
                && c.kept.stamp == kept.stamp
                && c.other.stamp == other.stamp
        });
        if known {
            return Ok(0);
        }
        self.store
            .add_sync_conflict(kind, uid, kept, other, self.now_secs())?;
        Ok(1)
    }

    /// A local item not synced yet. `None` when the replica already holds
    /// it as it is here (a built-in edited the same way elsewhere): it is
    /// only linked.
    fn new_item(&self, session: &Session, item: LocalItem) -> AppResult<Option<Outgoing>> {
        let uid = self.new_uid(&item)?;
        let version = match self.store.sync_item(item.kind, &uid)? {
            Some((_, known)) if known.body.as_deref() == Some(item.body.as_str()) => {
                self.store
                    .link_sync_item(item.kind, &uid, &item.local_id, &known)?;
                return Ok(None);
            }
            Some((_, known)) => known.replaced(self.next_stamp(&session.device)?, Some(item.body)),
            None => Version::new(self.next_stamp(&session.device)?, Some(item.body)),
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
    /// A PC not in it (removed, or never added) is not read.
    fn apply_batches(&self, session: &Session, report: &mut SyncReport) -> AppResult<()> {
        let mut pending: BTreeMap<DeviceId, Vec<(u64, String)>> = BTreeMap::new();
        for path in self.sync_remote.files(&session.dir, layout::DEVICES_DIR)? {
            if let Some((device, seq)) = layout::batch_seq(&path)
                && device != session.device
                && session.members.contains_key(device.as_str())
            {
                pending.entry(device).or_default().push((seq, path));
            }
        }
        for (device, mut batches) in pending {
            batches.sort();
            let mut read = self.store.sync_cursor(&device)?;
            let unread: Vec<(u64, String)> =
                batches.into_iter().filter(|(seq, _)| *seq > read).collect();
            for (seq, path) in unread {
                // Strictly in order: a batch missing for now (hidden, or not
                // pulled yet) stops this PC's batches until it is there.
                if seq != read + 1 {
                    break;
                }
                let Some(body) = self.open_signed(session, &device, &path)? else {
                    break;
                };
                let batch: Batch = serde_json::from_str(&body).map_err(|_| corrupt(&path))?;
                // A PC writes only its own versions.
                if batch.device != device
                    || batch.seq != seq
                    || batch
                        .changes
                        .iter()
                        .any(|c| c.version.stamp.device != device)
                {
                    return Err(corrupt(&path));
                }
                for change in &batch.changes {
                    self.apply_change(session, change, report)?;
                }
                self.store.set_sync_cursor(&device, seq)?;
                read = seq;
            }
        }
        Ok(())
    }

    fn apply_change(
        &self,
        session: &Session,
        change: &Change,
        report: &mut SyncReport,
    ) -> AppResult<()> {
        self.observe_stamp(&change.version.stamp)?;
        let known = self.store.sync_item(change.kind, &change.uid)?;
        match merge(known.as_ref().map(|(_, v)| v), &change.version) {
            Merge::Ignore => {}
            Merge::Apply => {
                self.store.put_sync_item(change)?;
                self.materialize(session, change)?;
                // Built on the version a conflict kept: settled elsewhere.
                for conflict in self.store.sync_conflicts()? {
                    if conflict.kind == change.kind && conflict.uid == change.uid {
                        self.store.delete_sync_conflict(conflict.id)?;
                    }
                }
                report.received += 1;
            }
            Merge::Conflict { incoming_wins } => {
                let (_, local) = known.expect("a conflict has a local version");
                let (kept, other) = if incoming_wins {
                    self.store.put_sync_item(change)?;
                    self.materialize(session, change)?;
                    report.received += 1;
                    (change.version.clone(), local)
                } else {
                    (local, change.version.clone())
                };
                if change.kind.asks_on_conflict() && other.body != kept.body {
                    report.conflicts +=
                        self.record_conflict(change.kind, &change.uid, &kept, &other)?;
                }
            }
        }
        Ok(())
    }

    /// Brings in synced items this PC never had: a project that became
    /// personal here, or one added later.
    fn bring_waiting(&self, session: &Session) -> AppResult<()> {
        for change in self.store.sync_items()? {
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
        let link: Option<SyncLink> = self.store.sync_link(change.kind, &change.uid)?;
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
                match change.kind {
                    ItemKind::Note => self.store.delete_note(row.unwrap_or(-1))?,
                    ItemKind::Skill => self.store.delete_skill(row.unwrap_or(-1))?,
                    ItemKind::Template => self.store.delete_template(row.unwrap_or(-1))?,
                    ItemKind::Theme => self.themes.delete(&link.local_id)?,
                }
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
                // An edited built-in here already has its row.
                let row = row.or_else(|| {
                    let builtin = body.builtin.as_ref()?;
                    self.store
                        .template_records()
                        .ok()?
                        .into_iter()
                        .find(|r| r.builtin.as_ref() == Some(builtin) && r.project == project)
                        .map(|r| r.id)
                });
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
        };
        self.store
            .link_sync_item(change.kind, &change.uid, &local_id, &change.version)?;
        Ok(true)
    }

    /// The sync's state for the status bar and Ajustes; `None` when it is
    /// not set up. Reads the clone only (no network): the trusted epoch's
    /// PCs, and their names as each signed them.
    pub fn sync_status(&self) -> AppResult<Option<SyncStatus>> {
        let Some(url) = self.sync_setting(URL)? else {
            return Ok(None);
        };
        let device = DeviceId(self.sync_setting(DEVICE)?.unwrap_or_default());
        let dir = self.sync_dir()?;
        let keys = self.keys()?;
        let trusted = self.sync_number(EPOCH)? as u32;
        let pins = self.pins()?;
        let mut devices = Vec::new();
        let trusted_file = self
            .epochs(&dir)?
            .into_iter()
            .find(|(n, f)| {
                *n == trusted && pins.get(n) == Some(&self.sync_crypto.digest(f.as_bytes()))
            })
            .map(|(_, f)| f);
        if let Some(file) = trusted_file {
            let (_, members) = self.sync_crypto.epoch_members(&file)?;
            let session = Session {
                dir: dir.clone(),
                device: device.clone(),
                me: keys.device.clone(),
                current: String::new(),
                projects: HashMap::new(),
                project_ids: HashMap::new(),
                members: members.iter().cloned().collect(),
                keys: keys.epochs.values().cloned().collect(),
            };
            for (id, _) in members.into_iter().filter(|(id, _)| id != RECOVERY_MEMBER) {
                let id = DeviceId(id);
                let info: Option<DeviceInfo> = self
                    .open_signed(&session, &id, &layout::device_info(&id))
                    .ok()
                    .flatten()
                    .and_then(|body| serde_json::from_str(&body).ok());
                devices.push(SyncDevice {
                    this_pc: id == device,
                    name: info.as_ref().map(|i| i.name.clone()).unwrap_or_default(),
                    last_sync: info.map(|i| i.last_sync),
                    id,
                });
            }
        }
        devices.sort_by_key(|d| !d.this_pc);
        Ok(Some(SyncStatus {
            url,
            device,
            last_sync: self.sync_setting(LAST_SYNC)?.and_then(|v| v.parse().ok()),
            devices,
            conflicts: self.store.sync_conflicts()?.len(),
        }))
    }

    pub fn sync_conflicts(&self) -> AppResult<Vec<SyncConflict>> {
        Ok(self.store.sync_conflicts()?)
    }

    /// Settles a conflict with a new version on top of the one in use: the
    /// other one (`use_other`) or the same again. It is written here and
    /// sent with the next sync, which also clears the conflict on the other
    /// PCs.
    pub fn resolve_sync_conflict(&self, id: i64, use_other: bool) -> AppResult<()> {
        let _guard = self.sync_guard();
        let conflict = self
            .store
            .sync_conflicts()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or(AppError::SyncConflictNotFound(id))?;
        let (scope, known) = self
            .store
            .sync_item(conflict.kind, &conflict.uid)?
            .ok_or(AppError::SyncConflictNotFound(id))?;
        let device = DeviceId(self.sync_setting(DEVICE)?.ok_or(AppError::SyncNotSetUp)?);
        let body = if use_other {
            conflict.other.body.clone()
        } else {
            known.body.clone()
        };
        let change = Change {
            kind: conflict.kind,
            uid: conflict.uid.clone(),
            scope,
            version: known.replaced(self.next_stamp(&device)?, body),
        };
        self.store.put_sync_item(&change)?;
        let keys = self.keys()?;
        let name_key = keys.epochs.values().next().cloned().unwrap_or_default();
        let SyncedProjects {
            projects,
            project_ids,
        } = self.synced_projects(&name_key)?;
        let session = Session {
            dir: self.sync_dir()?,
            device,
            me: keys.device.clone(),
            current: String::new(),
            projects,
            project_ids,
            members: HashMap::new(),
            keys: Vec::new(),
        };
        self.materialize(&session, &change)?;
        let mut outbox: Vec<Change> = self.sync_json(OUTBOX)?.unwrap_or_default();
        outbox.push(change);
        self.set_sync_setting(OUTBOX, &json(&outbox))?;
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
        ItemKind::Template | ItemKind::Theme => None,
    }
}

/// The repo's only plain file besides the key epochs.
fn readme() -> String {
    "# Hedy\n\nPersonal sync of Hedy (notes, skills, templates and themes), sealed with \
     XChaCha20-Poly1305 and signed on each PC before it is pushed. Nothing here can be read \
     without the owner's keys.\n"
        .into()
}

#[cfg(test)]
mod tests;
