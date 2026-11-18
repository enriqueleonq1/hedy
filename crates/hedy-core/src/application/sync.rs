//! Personal sync (docs/decisions.md §34): the same notes, skills, templates
//! and themes on every PC of the user, through a private git repo, sealed on
//! this PC before they leave it.
//!
//! A sync is: pull; check any new key epoch; turn what changed here since
//! the last sync into versions (comparing each item with the replica); apply
//! the other PCs' new batches ([`merge`]); write this PC's batch; push.
//! Nothing here reads a login or an API key, and only projects whose memory
//! is personal take part.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::ports::{SyncConflict, TemplateRecord};
use super::{AppError, AppResult, Hedy};
use crate::domain::sync::{
    Batch, Change, DeviceId, ItemKind, MemoryHome, Merge, NoteBody, RECOVERY_MEMBER, RepoIdentity,
    Scope, SkillBody, Stamp, TemplateBody, Version, layout, merge,
};
use crate::domain::theme::CustomTheme;
use crate::tr;

/// Settings of this PC's sync (plain, not secret).
const URL: &str = "sync.url";
const DEVICE: &str = "sync.device";
const NAME: &str = "sync.device_name";
/// The recovery key's public keys: the root every epoch is checked from.
const RECOVERY: &str = "sync.recovery";
/// The newest epoch checked and trusted.
const EPOCH: &str = "sync.epoch";
/// This PC's last batch.
const SEQ: &str = "sync.seq";
/// The newest stamp made or seen (JSON), so new ones are always later.
const CLOCK: &str = "sync.clock";
const LAST_SYNC: &str = "sync.last_sync";
/// When this PC last wrote its name and last sync to the repo.
const INFO_AT: &str = "sync.info_at";

/// This PC writes its name and last sync at most this often (seconds), so
/// a sync with nothing new does not make a commit.
const INFO_EVERY: i64 = 3600;

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

/// A PC's name and last sync, as it writes them to the repo (sealed).
#[derive(Serialize, Deserialize)]
struct DeviceInfo {
    name: String,
    last_sync: i64,
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
    /// The PCs of the newest key epoch, this one first.
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
    /// Every epoch key this PC has, oldest first.
    keys: Vec<String>,
    /// The newest one: what this PC seals with.
    current: String,
    /// Sealed project id → the project here.
    projects: HashMap<String, PathBuf>,
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

impl Hedy {
    fn sync_setting(&self, key: &str) -> AppResult<Option<String>> {
        Ok(self.store.get_setting(key)?)
    }

    fn set_sync_setting(&self, key: &str, value: &str) -> AppResult<()> {
        Ok(self.store.set_setting(key, Some(value))?)
    }

    fn sync_number(&self, key: &str) -> AppResult<u64> {
        Ok(self
            .sync_setting(key)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0))
    }

    /// The sync repo's clone, inside Hedy's data folder.
    fn sync_dir(&self) -> PathBuf {
        self.store
            .data_dir()
            .unwrap_or_default()
            .join("sync")
            .join("repo")
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

    /// The epoch files in the repo, by number.
    fn epochs(&self, dir: &Path) -> AppResult<Vec<(u32, String)>> {
        let mut epochs = Vec::new();
        for path in self.sync_remote.files(dir, layout::EPOCHS_DIR)? {
            let Some(bytes) = self.sync_remote.read(dir, &path)? else {
                continue;
            };
            let text = String::from_utf8(bytes).map_err(|_| corrupt(&path))?;
            let (number, _) = self.sync_crypto.epoch_members(&text)?;
            if path != layout::epoch(number) {
                return Err(corrupt(&path));
            }
            epochs.push((number, text));
        }
        epochs.sort_by_key(|(n, _)| *n);
        Ok(epochs)
    }

    /// Starts syncing with the empty repo at `url`: this PC's keys, a new
    /// recovery key and the first epoch, then a first sync. Returns the 12
    /// words, to show the user once.
    pub fn sync_setup(&self, url: &str, device_name: &str) -> AppResult<String> {
        let _lock = self.sync_lock.lock().unwrap();
        if self.sync_is_set_up()? {
            return Err(AppError::SyncAlreadySetUp);
        }
        let dir = self.sync_dir();
        self.sync_remote.open(url, &dir)?;
        if !self.sync_remote.files(&dir, layout::EPOCHS_DIR)?.is_empty() {
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
        self.save_keys(&Keys {
            device: me,
            epochs: BTreeMap::from([(1, key)]),
        })?;
        self.remember_sync(url, &device, device_name, &recovery_public, 1)?;
        self.sync_locked()?;
        Ok(words)
    }

    /// Joins the sync at `url` with the recovery key's `words`: checks every
    /// epoch from the recovery key, adds this PC in a new epoch, then syncs
    /// (which brings everything in).
    pub fn sync_join(&self, url: &str, words: &str, device_name: &str) -> AppResult<SyncReport> {
        let _lock = self.sync_lock.lock().unwrap();
        if self.sync_is_set_up()? {
            return Err(AppError::SyncAlreadySetUp);
        }
        let dir = self.sync_dir();
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
        let mut keys = BTreeMap::new();
        for (i, (number, file)) in epochs.iter().enumerate() {
            if i > 0 {
                self.sync_crypto.verify_next_epoch(&epochs[i - 1].1, file)?;
            }
            keys.insert(
                *number,
                self.sync_crypto
                    .open_epoch(file, RECOVERY_MEMBER, &recovery)?,
            );
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
        keys.insert(next, key);
        self.save_keys(&Keys {
            device: me,
            epochs: keys,
        })?;
        self.remember_sync(url, &device, device_name, &recovery_public, next)?;
        self.sync_locked()
    }

    fn remember_sync(
        &self,
        url: &str,
        device: &DeviceId,
        name: &str,
        recovery_public: &str,
        epoch: u32,
    ) -> AppResult<()> {
        self.set_sync_setting(URL, url)?;
        self.set_sync_setting(DEVICE, device.as_str())?;
        self.set_sync_setting(NAME, name.trim())?;
        self.set_sync_setting(RECOVERY, recovery_public)?;
        self.set_sync_setting(EPOCH, &epoch.to_string())?;
        self.set_sync_setting(SEQ, "0")
    }

    /// One sync, now.
    pub fn sync_now(&self) -> AppResult<SyncReport> {
        let _lock = self.sync_lock.lock().unwrap();
        self.sync_locked()
    }

    fn sync_locked(&self) -> AppResult<SyncReport> {
        let url = self.sync_setting(URL)?.ok_or(AppError::SyncNotSetUp)?;
        let device = DeviceId(self.sync_setting(DEVICE)?.ok_or(AppError::SyncNotSetUp)?);
        let dir = self.sync_dir();
        self.sync_remote.open(&url, &dir)?;
        let mut session = self.sync_session(dir, device)?;
        let mut report = SyncReport::default();

        // What changed here first, so the other PCs' versions meet it.
        let mine = self.local_changes(&session)?;
        report.sent = mine.len();
        self.apply_batches(&mut session, &mut report)?;

        let now = self.now_secs();
        if !mine.is_empty() {
            let seq = self.sync_number(SEQ)? + 1;
            let path = layout::batch(&session.device, seq);
            let batch = Batch {
                device: session.device.clone(),
                seq,
                changes: mine,
            };
            let sealed = self
                .sync_crypto
                .seal(&session.current, &path, json(&batch).as_bytes())?;
            self.sync_remote.write(&session.dir, &path, &sealed)?;
            self.set_sync_setting(SEQ, &seq.to_string())?;
        }
        let info_at = self.sync_number(INFO_AT)? as i64;
        if report.sent > 0 || now - info_at >= INFO_EVERY {
            let info = DeviceInfo {
                name: self.sync_setting(NAME)?.unwrap_or_default(),
                last_sync: now,
            };
            let path = layout::device_info(&session.device);
            let sealed = self
                .sync_crypto
                .seal(&session.current, &path, json(&info).as_bytes())?;
            self.sync_remote.write(&session.dir, &path, &sealed)?;
            self.set_sync_setting(INFO_AT, &now.to_string())?;
        }
        self.sync_remote.push(&session.dir, "hedy sync")?;
        self.set_sync_setting(LAST_SYNC, &now.to_string())?;
        Ok(report)
    }

    /// Checks any epoch newer than the trusted one (each signed by a member
    /// of the one before), keeps its key, and fails when the newest leaves
    /// this PC out.
    fn sync_session(&self, dir: PathBuf, device: DeviceId) -> AppResult<Session> {
        let mut keys = self.keys()?;
        let trusted = self.sync_number(EPOCH)? as u32;
        let epochs = self.epochs(&dir)?;
        let Some(mut previous) = epochs
            .iter()
            .find(|(n, _)| *n == trusted)
            .map(|(_, f)| f.clone())
        else {
            return Err(corrupt(&layout::epoch(trusted)));
        };
        let mut newest = trusted;
        for (number, file) in epochs.iter().filter(|(n, _)| *n > trusted) {
            self.sync_crypto.verify_next_epoch(&previous, file)?;
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
        let current = keys
            .epochs
            .get(&newest)
            .cloned()
            .ok_or(AppError::SyncRemoved)?;
        let all: Vec<String> = keys.epochs.values().cloned().collect();
        let projects = self.synced_projects(&all)?;
        Ok(Session {
            dir,
            device,
            keys: all,
            current,
            projects,
        })
    }

    /// Projects whose memory is personal, by their sealed id under every key
    /// (so batches sealed under an older epoch still find them).
    fn synced_projects(&self, keys: &[String]) -> AppResult<HashMap<String, PathBuf>> {
        let mut projects = HashMap::new();
        for project in self.store.list_projects()? {
            let Some(identity) = self.synced_identity(&project.path)? else {
                continue;
            };
            for key in keys {
                let id = self.sync_crypto.project_id(key, &identity.project_key())?;
                projects.insert(id, project.path.clone());
            }
        }
        Ok(projects)
    }

    /// Who owns the project, when its memory is personal and it has a
    /// remote to match it by on other PCs.
    fn synced_identity(&self, project: &Path) -> AppResult<Option<RepoIdentity>> {
        if !self
            .store
            .memory_home(project)?
            .is_some_and(|home| home.syncs())
        {
            return Ok(None);
        }
        let url = self.source.remote_url(project, "origin").ok().flatten();
        Ok(url.as_deref().and_then(RepoIdentity::from_url))
    }

    /// The scope a local item syncs under; `None` when it stays here.
    fn scope_of(&self, project: Option<&Path>, current: &str) -> AppResult<Option<Scope>> {
        let Some(project) = project else {
            return Ok(Some(Scope::Global));
        };
        let Some(identity) = self.synced_identity(project)? else {
            return Ok(None);
        };
        Ok(Some(Scope::Project(
            self.sync_crypto
                .project_id(current, &identity.project_key())?,
        )))
    }

    /// Every local item that syncs, as it is now.
    fn local_items(&self, current: &str) -> AppResult<Vec<LocalItem>> {
        let mut items = Vec::new();
        let mut places: Vec<Option<PathBuf>> = vec![None];
        places.extend(
            self.store
                .list_projects()?
                .into_iter()
                .map(|p| Some(p.path)),
        );
        for place in &places {
            let Some(scope) = self.scope_of(place.as_deref(), current)? else {
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
            let Some(scope) = self.scope_of(record.project.as_deref(), current)? else {
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
            items.push(LocalItem {
                kind: ItemKind::Theme,
                local_id: theme.id.clone(),
                scope: Scope::Global,
                body: json(&theme),
            });
        }
        Ok(items)
    }

    /// Whether the local item is still here (whatever its scope).
    fn local_exists(&self, kind: ItemKind, local_id: &str) -> AppResult<bool> {
        let row = || local_id.parse::<i64>().ok();
        Ok(match kind {
            ItemKind::Note => match row() {
                Some(id) => self.store.note(id)?.is_some(),
                None => false,
            },
            ItemKind::Skill => match row() {
                Some(id) => self.store.skill(id)?.is_some(),
                None => false,
            },
            ItemKind::Template => self
                .store
                .template_records()?
                .iter()
                .any(|r| Some(r.id) == row()),
            ItemKind::Theme => self.themes.get(local_id)?.is_some(),
        })
    }

    /// A new stamp, later than any made or seen here.
    fn next_stamp(&self, device: &DeviceId) -> AppResult<Stamp> {
        let last: Option<Stamp> = self
            .sync_setting(CLOCK)?
            .and_then(|t| serde_json::from_str(&t).ok());
        let stamp = Stamp::next(last.as_ref(), self.clock.now_ms(), device);
        self.set_sync_setting(CLOCK, &json(&stamp))?;
        Ok(stamp)
    }

    /// Keeps the clock ahead of a stamp from another PC.
    fn observe_stamp(&self, seen: &Stamp) -> AppResult<()> {
        let last: Option<Stamp> = self
            .sync_setting(CLOCK)?
            .and_then(|t| serde_json::from_str(&t).ok());
        if last.as_ref().is_none_or(|l| seen > l) {
            self.set_sync_setting(CLOCK, &json(seen))?;
        }
        Ok(())
    }

    /// What changed here since the last sync, as new versions (already in
    /// the replica): new and edited items, then deleted ones.
    fn local_changes(&self, session: &Session) -> AppResult<Vec<Change>> {
        let mut changes = Vec::new();
        for item in self.local_items(&session.current)? {
            let uid = match self.store.sync_uid(item.kind, &item.local_id)? {
                Some(uid) => uid,
                None => {
                    let uid = self.sync_crypto.random_hex(16)?;
                    self.store.link_sync_item(item.kind, &uid, &item.local_id)?;
                    uid
                }
            };
            let known = self.store.sync_item(item.kind, &uid)?;
            if let Some((scope, version)) = &known
                && *scope == item.scope
                && version.body.as_deref() == Some(item.body.as_str())
            {
                continue;
            }
            let stamp = self.next_stamp(&session.device)?;
            let version = match known {
                Some((_, version)) => version.replaced(stamp, Some(item.body)),
                None => Version::new(stamp, Some(item.body)),
            };
            let change = Change {
                kind: item.kind,
                uid,
                scope: item.scope,
                version,
            };
            self.store.put_sync_item(&change)?;
            changes.push(change);
        }
        for kind in KINDS {
            for (uid, local_id) in self.store.sync_links(kind)? {
                if self.local_exists(kind, &local_id)? {
                    continue;
                }
                self.store.unlink_sync_item(kind, &uid)?;
                let Some((scope, version)) = self.store.sync_item(kind, &uid)? else {
                    continue;
                };
                if version.is_deletion() {
                    continue;
                }
                let stamp = self.next_stamp(&session.device)?;
                let change = Change {
                    kind,
                    uid,
                    scope,
                    version: version.replaced(stamp, None),
                };
                self.store.put_sync_item(&change)?;
                changes.push(change);
            }
        }
        Ok(changes)
    }

    /// Reads the other PCs' batches this PC has not read yet, in order.
    fn apply_batches(&self, session: &mut Session, report: &mut SyncReport) -> AppResult<()> {
        let mut pending: BTreeMap<DeviceId, Vec<(u64, String)>> = BTreeMap::new();
        for path in self.sync_remote.files(&session.dir, layout::DEVICES_DIR)? {
            if let Some((device, seq)) = layout::batch_seq(&path)
                && device != session.device
            {
                pending.entry(device).or_default().push((seq, path));
            }
        }
        for (device, mut batches) in pending {
            batches.sort();
            let read = self.store.sync_cursor(&device)?;
            for (seq, path) in batches.into_iter().filter(|(seq, _)| *seq > read) {
                let Some(sealed) = self.sync_remote.read(&session.dir, &path)? else {
                    continue;
                };
                let plain = self.sync_crypto.open(&session.keys, &path, &sealed)?;
                let batch: Batch = serde_json::from_slice(&plain).map_err(|_| corrupt(&path))?;
                if batch.device != device || batch.seq != seq {
                    return Err(corrupt(&path));
                }
                for change in &batch.changes {
                    self.apply_change(session, change, report)?;
                }
                self.store.set_sync_cursor(&device, seq)?;
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
                if change.kind.asks_on_conflict() && other.body.is_some() && other.body != kept.body
                {
                    self.store.add_sync_conflict(
                        change.kind,
                        &change.uid,
                        &kept,
                        &other,
                        self.now_secs(),
                    )?;
                    report.conflicts += 1;
                }
            }
        }
        Ok(())
    }

    /// Writes a synced version into this PC's notes, skills, templates or
    /// themes. A project this PC does not have (or keeps local) waits in
    /// the replica.
    fn materialize(&self, session: &Session, change: &Change) -> AppResult<()> {
        let project = match &change.scope {
            Scope::Global => None,
            Scope::Project(id) => match session.projects.get(id) {
                Some(path) => Some(path.clone()),
                None => return Ok(()),
            },
        };
        let local = self.store.sync_local_id(change.kind, &change.uid)?;
        let row = local.as_deref().and_then(|l| l.parse::<i64>().ok());
        let Some(body) = &change.version.body else {
            if let Some(local) = &local {
                match change.kind {
                    ItemKind::Note => self.store.delete_note(row.unwrap_or(-1))?,
                    ItemKind::Skill => self.store.delete_skill(row.unwrap_or(-1))?,
                    ItemKind::Template => self.store.delete_template(row.unwrap_or(-1))?,
                    ItemKind::Theme => self.themes.delete(local)?,
                }
                self.store.unlink_sync_item(change.kind, &change.uid)?;
            }
            return Ok(());
        };
        let bad = || corrupt(&change.uid);
        let local_id = match change.kind {
            ItemKind::Note => {
                let body: NoteBody = serde_json::from_str(body).map_err(|_| bad())?;
                self.store
                    .write_synced_note(row, project.as_deref(), &body)?
                    .to_string()
            }
            ItemKind::Skill => {
                let body: SkillBody = serde_json::from_str(body).map_err(|_| bad())?;
                self.store
                    .write_synced_skill(row, project.as_deref(), &body)?
                    .to_string()
            }
            ItemKind::Template => {
                let body: TemplateBody = serde_json::from_str(body).map_err(|_| bad())?;
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
                let theme: CustomTheme = serde_json::from_str(body).map_err(|_| bad())?;
                self.themes.save(&theme)?;
                theme.id
            }
        };
        self.store
            .link_sync_item(change.kind, &change.uid, &local_id)
            .map_err(Into::into)
    }

    /// The sync's state for the status bar and Ajustes; `None` when it is
    /// not set up. Reads the PCs' names from the clone (no network).
    pub fn sync_status(&self) -> AppResult<Option<SyncStatus>> {
        let Some(url) = self.sync_setting(URL)? else {
            return Ok(None);
        };
        let device = DeviceId(self.sync_setting(DEVICE)?.unwrap_or_default());
        let dir = self.sync_dir();
        let keys: Vec<String> = self.keys()?.epochs.into_values().collect();
        let mut devices = Vec::new();
        if let Some((_, newest)) = self.epochs(&dir)?.pop() {
            let (_, members) = self.sync_crypto.epoch_members(&newest)?;
            for (id, _) in members.into_iter().filter(|(id, _)| id != RECOVERY_MEMBER) {
                let id = DeviceId(id);
                let path = layout::device_info(&id);
                let info: Option<DeviceInfo> = self
                    .sync_remote
                    .read(&dir, &path)?
                    .and_then(|sealed| self.sync_crypto.open(&keys, &path, &sealed).ok())
                    .and_then(|plain| serde_json::from_slice(&plain).ok());
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

    /// Settles a conflict: keep the version in use, or use the other one
    /// (written here, so the next sync sends it on top of the kept one).
    pub fn resolve_sync_conflict(&self, id: i64, use_other: bool) -> AppResult<()> {
        let _lock = self.sync_lock.lock().unwrap();
        let conflict = self
            .store
            .sync_conflicts()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or(AppError::SyncConflictNotFound(id))?;
        if use_other {
            let (scope, _) = self
                .store
                .sync_item(conflict.kind, &conflict.uid)?
                .ok_or(AppError::SyncConflictNotFound(id))?;
            let keys: Vec<String> = self.keys()?.epochs.into_values().collect();
            let session = Session {
                dir: self.sync_dir(),
                device: DeviceId(self.sync_setting(DEVICE)?.unwrap_or_default()),
                current: keys.last().cloned().unwrap_or_default(),
                projects: self.synced_projects(&keys)?,
                keys,
            };
            let change = Change {
                kind: conflict.kind,
                uid: conflict.uid.clone(),
                scope,
                version: conflict.other.clone(),
            };
            self.materialize(&session, &change)?;
        }
        Ok(self.store.delete_sync_conflict(id)?)
    }

    /// Where `project`'s memory lives (`None`: not decided, it stays here).
    pub fn set_memory_home(&self, project: &Path, home: Option<&MemoryHome>) -> AppResult<()> {
        let project = self.project_at(project)?;
        Ok(self.store.set_memory_home(&project.path, home)?)
    }

    pub fn memory_home(&self, project: &Path) -> AppResult<Option<MemoryHome>> {
        let project = self.project_at(project)?;
        Ok(self.store.memory_home(&project.path)?)
    }
}

/// The repo's only plain file besides the key epochs.
fn readme() -> String {
    "# Hedy\n\nPersonal sync of Hedy (notes, skills, templates and themes), sealed with \
     XChaCha20-Poly1305 on each PC before it is pushed. Nothing here can be read without \
     the owner's keys.\n"
        .into()
}

#[cfg(test)]
mod tests;
