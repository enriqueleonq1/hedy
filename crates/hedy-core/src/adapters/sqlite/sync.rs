//! Personal sync's local tables (docs/decisions.md §34): the replica of
//! synced items, their links to local rows, cursors, conflicts and each
//! project's memory home.

use std::path::Path;

use rusqlite::{OptionalExtension, params};

use super::{SqliteStore, skill_status_str, status_str};
use crate::application::ports::{PortResult, SyncConflict, SyncLink, SyncRepository};
use crate::domain::sync::{
    Change, DeviceId, ItemKind, MemoryHome, NoteBody, Scope, SkillBody, Version,
};

fn kind_str(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Note => "note",
        ItemKind::Skill => "skill",
        ItemKind::Template => "template",
        ItemKind::Theme => "theme",
    }
}

fn kind_from(text: &str) -> Option<ItemKind> {
    [
        ItemKind::Note,
        ItemKind::Skill,
        ItemKind::Template,
        ItemKind::Theme,
    ]
    .into_iter()
    .find(|k| kind_str(*k) == text)
}

/// JSON a column holds, or a conversion error rusqlite reports.
fn from_json<T: serde::de::DeserializeOwned>(index: usize, text: &str) -> rusqlite::Result<T> {
    serde_json::from_str(text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("sync records serialize")
}

const LINK_COLUMNS: &str = "uid, local_id, base_stamp, base_body";

fn link_from_row(r: &rusqlite::Row) -> rusqlite::Result<SyncLink> {
    Ok(SyncLink {
        uid: r.get(0)?,
        local_id: r.get(1)?,
        base_stamp: from_json(2, &r.get::<_, String>(2)?)?,
        base_body: r.get(3)?,
    })
}

fn path_text(path: Option<&Path>) -> Option<String> {
    path.map(|p| p.to_string_lossy().into_owned())
}

impl SyncRepository for SqliteStore {
    fn sync_item(&self, kind: ItemKind, uid: &str) -> PortResult<Option<(Scope, Version)>> {
        self.with(|c| {
            c.query_row(
                "SELECT scope, version FROM sync_items WHERE kind = ?1 AND uid = ?2",
                params![kind_str(kind), uid],
                |r| {
                    Ok((
                        from_json(0, &r.get::<_, String>(0)?)?,
                        from_json(1, &r.get::<_, String>(1)?)?,
                    ))
                },
            )
            .optional()
        })
    }

    fn sync_items(&self) -> PortResult<Vec<Change>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT kind, uid, scope, version FROM sync_items")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?;
            let mut changes = Vec::new();
            for row in rows {
                let (kind, uid, scope, version) = row?;
                // A kind from a newer Hedy is left alone.
                let Some(kind) = kind_from(&kind) else {
                    continue;
                };
                changes.push(Change {
                    kind,
                    uid,
                    scope: from_json(2, &scope)?,
                    version: from_json(3, &version)?,
                });
            }
            Ok(changes)
        })
    }

    fn put_sync_item(&self, change: &Change) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO sync_items (kind, uid, scope, version) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (kind, uid) DO UPDATE SET scope = ?3, version = ?4",
                params![
                    kind_str(change.kind),
                    change.uid,
                    to_json(&change.scope),
                    to_json(&change.version)
                ],
            )
            .map(drop)
        })
    }

    fn sync_link(&self, kind: ItemKind, uid: &str) -> PortResult<Option<SyncLink>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {LINK_COLUMNS} FROM sync_links WHERE kind = ?1 AND uid = ?2"),
                params![kind_str(kind), uid],
                link_from_row,
            )
            .optional()
        })
    }

    fn sync_link_of(&self, kind: ItemKind, local_id: &str) -> PortResult<Option<SyncLink>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {LINK_COLUMNS} FROM sync_links WHERE kind = ?1 AND local_id = ?2"),
                params![kind_str(kind), local_id],
                link_from_row,
            )
            .optional()
        })
    }

    fn sync_links(&self, kind: ItemKind) -> PortResult<Vec<SyncLink>> {
        self.with(|c| {
            c.prepare(&format!(
                "SELECT {LINK_COLUMNS} FROM sync_links WHERE kind = ?1 ORDER BY uid"
            ))?
            .query_map([kind_str(kind)], link_from_row)?
            .collect()
        })
    }

    fn link_sync_item(
        &self,
        kind: ItemKind,
        uid: &str,
        local_id: &str,
        base: &Version,
    ) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "DELETE FROM sync_links WHERE kind = ?1 AND local_id = ?2 AND uid != ?3",
                params![kind_str(kind), local_id, uid],
            )?;
            c.execute(
                "INSERT INTO sync_links (kind, uid, local_id, base_stamp, base_body)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (kind, uid) DO UPDATE SET local_id = ?3, base_stamp = ?4,
                     base_body = ?5",
                params![
                    kind_str(kind),
                    uid,
                    local_id,
                    to_json(&base.stamp),
                    base.body
                ],
            )
            .map(drop)
        })
    }

    fn unlink_sync_item(&self, kind: ItemKind, uid: &str) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "DELETE FROM sync_links WHERE kind = ?1 AND uid = ?2",
                params![kind_str(kind), uid],
            )
            .map(drop)
        })
    }

    fn sync_cursor(&self, device: &DeviceId) -> PortResult<u64> {
        self.with(|c| {
            c.query_row(
                "SELECT seq FROM sync_cursors WHERE device = ?1",
                [device.as_str()],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map(|seq| seq.unwrap_or(0).max(0) as u64)
        })
    }

    fn set_sync_cursor(&self, device: &DeviceId, seq: u64) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO sync_cursors (device, seq) VALUES (?1, ?2)
                 ON CONFLICT (device) DO UPDATE SET seq = ?2",
                params![device.as_str(), seq as i64],
            )
            .map(drop)
        })
    }

    fn add_sync_conflict(
        &self,
        kind: ItemKind,
        uid: &str,
        kept: &Version,
        other: &Version,
        at: i64,
    ) -> PortResult<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO sync_conflicts (kind, uid, kept, other, at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![kind_str(kind), uid, to_json(kept), to_json(other), at],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    fn sync_conflicts(&self) -> PortResult<Vec<SyncConflict>> {
        self.with(|c| {
            let mut stmt =
                c.prepare("SELECT id, kind, uid, kept, other, at FROM sync_conflicts ORDER BY id")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            })?;
            let mut conflicts = Vec::new();
            for row in rows {
                let (id, kind, uid, kept, other, at) = row?;
                let Some(kind) = kind_from(&kind) else {
                    continue;
                };
                conflicts.push(SyncConflict {
                    id,
                    kind,
                    uid,
                    kept: from_json(3, &kept)?,
                    other: from_json(4, &other)?,
                    at,
                });
            }
            Ok(conflicts)
        })
    }

    fn delete_sync_conflict(&self, id: i64) -> PortResult<()> {
        self.with(|c| {
            c.execute("DELETE FROM sync_conflicts WHERE id = ?1", [id])
                .map(drop)
        })
    }

    fn memory_home(&self, project: &Path) -> PortResult<Option<MemoryHome>> {
        self.with(|c| {
            c.query_row(
                "SELECT memory_home FROM projects WHERE path = ?1",
                [project.to_string_lossy()],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            // A value this version cannot read keeps the memory here.
            .map(|home| {
                home.flatten()
                    .map(|text| MemoryHome::from_stored(&text).unwrap_or(MemoryHome::Local))
            })
        })
    }

    fn set_memory_home(&self, project: &Path, home: Option<&MemoryHome>) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "UPDATE projects SET memory_home = ?2 WHERE path = ?1",
                params![project.to_string_lossy(), home.map(MemoryHome::as_stored)],
            )
            .map(drop)
        })
    }

    fn write_synced_note(
        &self,
        id: Option<i64>,
        project: Option<&Path>,
        body: &NoteBody,
    ) -> PortResult<i64> {
        self.with(|c| {
            let values = params![
                path_text(project),
                body.text,
                body.category.as_str(),
                body.author,
                status_str(body.status),
                body.created_at,
                body.topic,
                body.updated_at,
                id,
            ];
            let updated = match id {
                Some(_) => c.execute(
                    "UPDATE notes SET project = ?1, text = ?2, category = ?3, author = ?4,
                         status = ?5, created_at = ?6, topic = ?7, updated_at = ?8
                     WHERE id = ?9",
                    values,
                )?,
                None => 0,
            };
            if updated == 1 {
                return Ok(id.expect("an update had an id"));
            }
            c.execute(
                "INSERT INTO notes (project, text, category, author, status, created_at, topic,
                                    updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    path_text(project),
                    body.text,
                    body.category.as_str(),
                    body.author,
                    status_str(body.status),
                    body.created_at,
                    body.topic,
                    body.updated_at,
                ],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    fn write_synced_skill(
        &self,
        id: Option<i64>,
        project: Option<&Path>,
        body: &SkillBody,
    ) -> PortResult<i64> {
        self.with(|c| {
            let updated = match id {
                Some(id) => c.execute(
                    "UPDATE skills SET project = ?1, name = ?2, description = ?3, body = ?4,
                         status = ?5, author = ?6, created_at = ?7, updated_at = ?8
                     WHERE id = ?9",
                    params![
                        path_text(project),
                        body.name,
                        body.description,
                        body.body,
                        skill_status_str(body.status),
                        body.author,
                        body.created_at,
                        body.updated_at,
                        id,
                    ],
                )?,
                None => 0,
            };
            if updated == 1 {
                return Ok(id.expect("an update had an id"));
            }
            c.execute(
                "INSERT INTO skills (project, name, description, body, status, author,
                                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    path_text(project),
                    body.name,
                    body.description,
                    body.body,
                    skill_status_str(body.status),
                    body.author,
                    body.created_at,
                    body.updated_at,
                ],
            )?;
            Ok(c.last_insert_rowid())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{MemoryRepository, ProjectRepository, SkillRepository};
    use crate::domain::sync::Stamp;
    use crate::domain::{Category, NoteStatus, Project, SkillStatus};

    fn store() -> (tempfile::TempDir, SqliteStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&tmp.path().join("hedy.db")).unwrap();
        (tmp, store)
    }

    fn version(ms: i64) -> Version {
        Version::new(
            Stamp {
                ms,
                counter: 0,
                device: DeviceId::from_bytes([1; 8]),
            },
            Some("{}".into()),
        )
    }

    #[test]
    fn keeps_the_replica_links_cursors_and_conflicts() {
        let (_tmp, store) = store();
        let change = Change {
            kind: ItemKind::Note,
            uid: "u1".into(),
            scope: Scope::Project("p1".into()),
            version: version(1),
        };
        assert_eq!(store.sync_item(ItemKind::Note, "u1").unwrap(), None);
        store.put_sync_item(&change).unwrap();
        let newer = Change {
            version: version(2),
            ..change.clone()
        };
        store.put_sync_item(&newer).unwrap();
        assert_eq!(
            store.sync_item(ItemKind::Note, "u1").unwrap(),
            Some((Scope::Project("p1".into()), version(2)))
        );
        assert_eq!(store.sync_items().unwrap(), vec![newer]);

        store
            .link_sync_item(ItemKind::Note, "u1", "7", &version(1))
            .unwrap();
        // Relinking a local row moves it to the new uid.
        store
            .link_sync_item(ItemKind::Note, "u2", "7", &version(2))
            .unwrap();
        let link = store.sync_link_of(ItemKind::Note, "7").unwrap().unwrap();
        assert_eq!((link.uid.as_str(), link.base_stamp.ms), ("u2", 2));
        assert_eq!(link.base_body.as_deref(), Some("{}"));
        assert_eq!(store.sync_link(ItemKind::Note, "u1").unwrap(), None);
        assert_eq!(store.sync_link_of(ItemKind::Skill, "7").unwrap(), None);
        assert_eq!(store.sync_links(ItemKind::Note).unwrap(), [link]);
        store.unlink_sync_item(ItemKind::Note, "u2").unwrap();
        assert!(store.sync_links(ItemKind::Note).unwrap().is_empty());

        let pc = DeviceId::from_bytes([2; 8]);
        assert_eq!(store.sync_cursor(&pc).unwrap(), 0);
        store.set_sync_cursor(&pc, 4).unwrap();
        store.set_sync_cursor(&pc, 5).unwrap();
        assert_eq!(store.sync_cursor(&pc).unwrap(), 5);

        let id = store
            .add_sync_conflict(ItemKind::Skill, "s1", &version(3), &version(2), 99)
            .unwrap();
        let conflicts = store.sync_conflicts().unwrap();
        assert_eq!((conflicts[0].id, conflicts[0].kind), (id, ItemKind::Skill));
        assert_eq!(conflicts[0].other, version(2));
        store.delete_sync_conflict(id).unwrap();
        assert!(store.sync_conflicts().unwrap().is_empty());
    }

    #[test]
    fn writes_synced_notes_and_skills_and_memory_homes() {
        let (tmp, store) = store();
        let project = tmp.path().join("tienda");
        store.save_project(&Project::new(project.clone())).unwrap();
        assert_eq!(store.memory_home(&project).unwrap(), None);
        store
            .set_memory_home(&project, Some(&MemoryHome::Shared("empresa-org".into())))
            .unwrap();
        assert_eq!(
            store.memory_home(&project).unwrap(),
            Some(MemoryHome::Shared("empresa-org".into()))
        );
        store.set_memory_home(&project, None).unwrap();
        assert_eq!(store.memory_home(&project).unwrap(), None);
        // An owner left empty does not read back as "not decided".
        store
            .set_memory_home(&project, Some(&MemoryHome::Shared(String::new())))
            .unwrap();
        assert_eq!(
            store.memory_home(&project).unwrap(),
            Some(MemoryHome::Local)
        );

        let mut body = NoteBody {
            text: "USD 12.50".into(),
            category: Category::Convention,
            author: "Claude".into(),
            status: NoteStatus::Proposed,
            created_at: 10,
            topic: Some("precios".into()),
            updated_at: None,
        };
        let id = store
            .write_synced_note(None, Some(&project), &body)
            .unwrap();
        body.status = NoteStatus::Approved;
        body.updated_at = Some(20);
        assert_eq!(
            store
                .write_synced_note(Some(id), Some(&project), &body)
                .unwrap(),
            id
        );
        let note = store.note(id).unwrap().unwrap();
        assert_eq!(
            (note.status, note.updated_at),
            (NoteStatus::Approved, Some(20))
        );
        assert_eq!(note.project.as_deref(), Some(project.as_path()));
        // An id that is gone adds the note anew.
        assert_ne!(
            store.write_synced_note(Some(999), None, &body).unwrap(),
            999
        );

        let skill = SkillBody {
            name: "deploy".into(),
            description: "Cómo publicar".into(),
            body: "1. Publica".into(),
            status: SkillStatus::Approved,
            author: "Claude".into(),
            created_at: 5,
            updated_at: None,
        };
        let sid = store.write_synced_skill(None, None, &skill).unwrap();
        let saved = store.skill(sid).unwrap().unwrap();
        assert_eq!((saved.name.as_str(), saved.project), ("deploy", None));
    }
}
