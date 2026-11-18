//! In-memory fakes for personal sync: the store's sync tables and a sync
//! "server" several fixtures (PCs) share.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::MemoryStore;
use crate::application::ports::{PortResult, SyncConflict, SyncLink, SyncRemote, SyncRepository};
use crate::domain::sync::{
    Change, DeviceId, ItemKind, MemoryHome, NoteBody, Scope, SkillBody, Version,
};
use crate::domain::{Note, Skill};

#[derive(Default)]
pub struct SyncTables {
    items: BTreeMap<(ItemKind, String), Change>,
    links: BTreeMap<(ItemKind, String), SyncLink>,
    cursors: BTreeMap<DeviceId, u64>,
    conflicts: Vec<SyncConflict>,
    homes: BTreeMap<PathBuf, MemoryHome>,
}

impl SyncRepository for MemoryStore {
    fn sync_item(&self, kind: ItemKind, uid: &str) -> PortResult<Option<(Scope, Version)>> {
        let tables = self.sync.lock().unwrap();
        Ok(tables
            .items
            .get(&(kind, uid.to_owned()))
            .map(|c| (c.scope.clone(), c.version.clone())))
    }
    fn sync_items(&self) -> PortResult<Vec<Change>> {
        Ok(self.sync.lock().unwrap().items.values().cloned().collect())
    }
    fn put_sync_item(&self, change: &Change) -> PortResult<()> {
        self.sync
            .lock()
            .unwrap()
            .items
            .insert((change.kind, change.uid.clone()), change.clone());
        Ok(())
    }
    fn sync_link(&self, kind: ItemKind, uid: &str) -> PortResult<Option<SyncLink>> {
        let tables = self.sync.lock().unwrap();
        Ok(tables.links.get(&(kind, uid.to_owned())).cloned())
    }
    fn sync_link_of(&self, kind: ItemKind, local_id: &str) -> PortResult<Option<SyncLink>> {
        let tables = self.sync.lock().unwrap();
        Ok(tables
            .links
            .iter()
            .find(|((k, _), link)| *k == kind && link.local_id == local_id)
            .map(|(_, link)| link.clone()))
    }
    fn sync_links(&self, kind: ItemKind) -> PortResult<Vec<SyncLink>> {
        let tables = self.sync.lock().unwrap();
        Ok(tables
            .links
            .iter()
            .filter(|((k, _), _)| *k == kind)
            .map(|(_, link)| link.clone())
            .collect())
    }
    fn link_sync_item(
        &self,
        kind: ItemKind,
        uid: &str,
        local_id: &str,
        base: &Version,
    ) -> PortResult<()> {
        let mut tables = self.sync.lock().unwrap();
        tables
            .links
            .retain(|(k, u), link| *k != kind || link.local_id != local_id || u == uid);
        tables.links.insert(
            (kind, uid.to_owned()),
            SyncLink {
                uid: uid.to_owned(),
                local_id: local_id.to_owned(),
                base_stamp: base.stamp.clone(),
                base_body: base.body.clone(),
            },
        );
        Ok(())
    }
    fn unlink_sync_item(&self, kind: ItemKind, uid: &str) -> PortResult<()> {
        self.sync
            .lock()
            .unwrap()
            .links
            .remove(&(kind, uid.to_owned()));
        Ok(())
    }
    fn sync_cursor(&self, device: &DeviceId) -> PortResult<u64> {
        Ok(self
            .sync
            .lock()
            .unwrap()
            .cursors
            .get(device)
            .copied()
            .unwrap_or(0))
    }
    fn set_sync_cursor(&self, device: &DeviceId, seq: u64) -> PortResult<()> {
        self.sync
            .lock()
            .unwrap()
            .cursors
            .insert(device.clone(), seq);
        Ok(())
    }
    fn add_sync_conflict(
        &self,
        kind: ItemKind,
        uid: &str,
        kept: &Version,
        other: &Version,
        at: i64,
    ) -> PortResult<i64> {
        let mut tables = self.sync.lock().unwrap();
        let id = tables.conflicts.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        tables.conflicts.push(SyncConflict {
            id,
            kind,
            uid: uid.to_owned(),
            kept: kept.clone(),
            other: other.clone(),
            at,
        });
        Ok(id)
    }
    fn sync_conflicts(&self) -> PortResult<Vec<SyncConflict>> {
        Ok(self.sync.lock().unwrap().conflicts.clone())
    }
    fn delete_sync_conflict(&self, id: i64) -> PortResult<()> {
        self.sync.lock().unwrap().conflicts.retain(|c| c.id != id);
        Ok(())
    }
    fn clear_sync_state(&self) -> PortResult<()> {
        let mut tables = self.sync.lock().unwrap();
        let homes = std::mem::take(&mut tables.homes);
        *tables = SyncTables {
            homes,
            ..SyncTables::default()
        };
        Ok(())
    }
    fn memory_home(&self, project: &Path) -> PortResult<Option<MemoryHome>> {
        Ok(self.sync.lock().unwrap().homes.get(project).cloned())
    }
    fn set_memory_home(&self, project: &Path, home: Option<&MemoryHome>) -> PortResult<()> {
        let mut tables = self.sync.lock().unwrap();
        match home {
            Some(home) => tables.homes.insert(project.to_owned(), home.clone()),
            None => tables.homes.remove(project),
        };
        Ok(())
    }
    fn write_synced_note(
        &self,
        id: Option<i64>,
        project: Option<&Path>,
        body: &NoteBody,
    ) -> PortResult<i64> {
        let mut notes = self.notes.lock().unwrap();
        let id = id.unwrap_or_else(|| notes.iter().map(|n| n.id).max().unwrap_or(0) + 1);
        let (uses, last_used) = notes
            .iter()
            .find(|n| n.id == id)
            .map_or((0, None), |n| (n.uses, n.last_used));
        notes.retain(|n| n.id != id);
        notes.push(Note {
            id,
            project: project.map(Path::to_owned),
            text: body.text.clone(),
            category: body.category,
            author: body.author.clone(),
            status: body.status,
            created_at: body.created_at,
            uses,
            last_used,
            topic: body.topic.clone(),
            updated_at: body.updated_at,
        });
        notes.sort_by_key(|n| n.id);
        Ok(id)
    }
    fn write_synced_skill(
        &self,
        id: Option<i64>,
        project: Option<&Path>,
        body: &SkillBody,
    ) -> PortResult<i64> {
        let mut skills = self.skills.lock().unwrap();
        let id = id.unwrap_or_else(|| skills.iter().map(|s| s.id).max().unwrap_or(0) + 1);
        skills.retain(|s| s.id != id);
        skills.push(Skill {
            id,
            name: body.name.clone(),
            description: body.description.clone(),
            body: body.body.clone(),
            project: project.map(Path::to_owned),
            status: body.status,
            author: body.author.clone(),
            created_at: body.created_at,
            updated_at: body.updated_at,
            source_task: None,
        });
        skills.sort_by_key(|s| s.id);
        Ok(id)
    }
}

/// Files of a sync repo, shared by every PC (fixture) that syncs through it.
#[derive(Clone, Default)]
pub struct SyncServer(Arc<Mutex<BTreeMap<String, Vec<u8>>>>);

impl SyncServer {
    pub fn paths(&self) -> Vec<String> {
        self.0.lock().unwrap().keys().cloned().collect()
    }

    pub fn file(&self, path: &str) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(path).cloned()
    }
}

/// One PC's working copy of a [`SyncServer`]: what it pulled, plus what it
/// wrote since its last push.
pub struct FakeSyncRemote {
    server: SyncServer,
    local: Mutex<BTreeMap<String, Vec<u8>>>,
    /// Written (`Some`) or removed (`None`) since the last push.
    pending: Mutex<BTreeMap<String, Option<Vec<u8>>>>,
    pushes: Mutex<Vec<String>>,
    /// Pushes to let through before one fails (the network went down).
    fail_push: Mutex<Option<usize>>,
}

impl FakeSyncRemote {
    pub fn new(server: SyncServer) -> Self {
        Self {
            server,
            local: Mutex::default(),
            pending: Mutex::default(),
            pushes: Mutex::default(),
            fail_push: Mutex::default(),
        }
    }

    pub fn fail_next_push(&self) {
        self.fail_push_after(0);
    }

    /// Lets `ok` pushes through, then fails one.
    pub fn fail_push_after(&self, ok: usize) {
        *self.fail_push.lock().unwrap() = Some(ok);
    }

    /// The commit messages pushed so far.
    pub fn pushes(&self) -> Vec<String> {
        self.pushes.lock().unwrap().clone()
    }
}

impl SyncRemote for FakeSyncRemote {
    fn open(&self, _url: &str, dir: &Path) -> PortResult<()> {
        self.pull(dir)
    }
    fn pull(&self, _dir: &Path) -> PortResult<()> {
        let mut local = self.server.0.lock().unwrap().clone();
        for (path, file) in self.pending.lock().unwrap().iter() {
            match file {
                Some(bytes) => local.insert(path.clone(), bytes.clone()),
                None => local.remove(path),
            };
        }
        *self.local.lock().unwrap() = local;
        Ok(())
    }
    fn files(&self, _dir: &Path, prefix: &str) -> PortResult<Vec<String>> {
        Ok(self
            .local
            .lock()
            .unwrap()
            .keys()
            .filter(|p| p.starts_with(prefix))
            .cloned()
            .collect())
    }
    fn read(&self, _dir: &Path, path: &str) -> PortResult<Option<Vec<u8>>> {
        Ok(self.local.lock().unwrap().get(path).cloned())
    }
    fn write(&self, _dir: &Path, path: &str, bytes: &[u8]) -> PortResult<()> {
        self.local
            .lock()
            .unwrap()
            .insert(path.into(), bytes.to_vec());
        self.pending
            .lock()
            .unwrap()
            .insert(path.into(), Some(bytes.to_vec()));
        Ok(())
    }
    fn remove(&self, _dir: &Path, path: &str) -> PortResult<()> {
        self.local.lock().unwrap().remove(path);
        self.pending.lock().unwrap().insert(path.into(), None);
        Ok(())
    }
    fn discard_local(&self, dir: &Path) -> PortResult<()> {
        self.pending.lock().unwrap().clear();
        self.pull(dir)
    }
    fn push(&self, _dir: &Path, message: &str) -> PortResult<()> {
        {
            let mut fail = self.fail_push.lock().unwrap();
            match *fail {
                Some(0) => {
                    *fail = None;
                    return Err(crate::application::ports::PortError::new("sin conexión"));
                }
                Some(ok) => *fail = Some(ok - 1),
                None => {}
            }
        }
        let pending = std::mem::take(&mut *self.pending.lock().unwrap());
        if pending.is_empty() {
            return Ok(());
        }
        let mut server = self.server.0.lock().unwrap();
        for (path, file) in pending {
            match file {
                Some(bytes) => server.insert(path, bytes),
                None => server.remove(&path),
            };
        }
        self.pushes.lock().unwrap().push(message.to_owned());
        Ok(())
    }
}
