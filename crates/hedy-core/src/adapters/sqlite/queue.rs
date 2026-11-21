//! The queue of messages written while an agent works.

use rusqlite::{OptionalExtension, Row, params};

use super::SqliteStore;
use crate::application::ports::{PortResult, QueueRepository};
use crate::domain::{HoldReason, QueuedMessage, TaskId};

const COLUMNS: &str = "id, task_id, text, created_at, held";

fn queued_from_row(row: &Row) -> rusqlite::Result<QueuedMessage> {
    Ok(QueuedMessage {
        id: row.get(0)?,
        task: TaskId::new(row.get::<_, String>(1)?),
        text: row.get(2)?,
        created_at: row.get(3)?,
        held: row
            .get::<_, Option<String>>(4)?
            .as_deref()
            .and_then(HoldReason::parse),
    })
}

impl QueueRepository for SqliteStore {
    fn add_queued(&self, task: &TaskId, text: &str, at: i64) -> PortResult<QueuedMessage> {
        let id = self.with(|c| {
            c.execute(
                "INSERT INTO queued_messages (task_id, text, created_at) VALUES (?1, ?2, ?3)",
                params![task.as_str(), text, at],
            )?;
            Ok(c.last_insert_rowid())
        })?;
        Ok(QueuedMessage {
            id,
            task: task.clone(),
            text: text.to_owned(),
            created_at: at,
            held: None,
        })
    }

    fn queued(&self, task: &TaskId) -> PortResult<Vec<QueuedMessage>> {
        self.with(|c| {
            c.prepare(&format!(
                "SELECT {COLUMNS} FROM queued_messages WHERE task_id = ?1 ORDER BY id"
            ))?
            .query_map([task.as_str()], queued_from_row)?
            .collect()
        })
    }

    fn queued_message(&self, id: i64) -> PortResult<Option<QueuedMessage>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {COLUMNS} FROM queued_messages WHERE id = ?1"),
                [id],
                queued_from_row,
            )
            .optional()
        })
    }

    fn update_queued(&self, id: i64, text: &str) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "UPDATE queued_messages SET text = ?2 WHERE id = ?1",
                params![id, text],
            )
            .map(drop)
        })
    }

    fn delete_queued(&self, ids: &[i64]) -> PortResult<()> {
        self.with(|c| {
            let mut delete = c.prepare("DELETE FROM queued_messages WHERE id = ?1")?;
            for id in ids {
                delete.execute([id])?;
            }
            Ok(())
        })
    }

    fn hold_queue(&self, task: &TaskId, reason: Option<HoldReason>) -> PortResult<()> {
        self.with(|c| {
            match reason {
                Some(reason) => c.execute(
                    "UPDATE queued_messages SET held = ?2 WHERE task_id = ?1 AND held IS NULL",
                    params![task.as_str(), reason.as_str()],
                ),
                None => c.execute(
                    "UPDATE queued_messages SET held = NULL WHERE task_id = ?1",
                    [task.as_str()],
                ),
            }
            .map(drop)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::application::ports::TaskRepository;
    use crate::domain::{AccountId, Task, TaskMode, TaskStatus};

    fn store_with_task(id: &str) -> SqliteStore {
        let store = SqliteStore::in_memory().unwrap();
        store
            .save_task(&Task {
                images: Vec::new(),
                id: TaskId::new(id),
                project: PathBuf::from("/code/x"),
                mode: TaskMode::Project,
                title: "x".into(),
                prompt: "x".into(),
                account: AccountId::new("personal").unwrap(),
                model: None,
                workspace: None,
                scratch: None,
                permissions: Default::default(),
                flow: None,
                session_id: None,
                status: TaskStatus::Running,
                created_at: 1,
                stages: vec![],
                stage: 0,
                plan: None,
                merge: None,
                wait: None,
                conversation: false,
                ship: None,
                origin: None,
                code_map: None,
            })
            .unwrap();
        store
    }

    #[test]
    fn queue_round_trip_in_order_with_holds() {
        let store = store_with_task("t1");
        let task = TaskId::new("t1");
        let a = store.add_queued(&task, "y los tests", 10).unwrap();
        let b = store.add_queued(&task, "usa pytest", 11).unwrap();
        assert_eq!(store.queued(&task).unwrap(), vec![a.clone(), b.clone()]);

        store.update_queued(b.id, "usa pytest -q").unwrap();
        assert_eq!(
            store.queued_message(b.id).unwrap().unwrap().text,
            "usa pytest -q"
        );

        store.hold_queue(&task, Some(HoldReason::Limit)).unwrap();
        let c = store.add_queued(&task, "otro", 12).unwrap();
        store.hold_queue(&task, Some(HoldReason::Stopped)).unwrap();
        let held: Vec<_> = store
            .queued(&task)
            .unwrap()
            .into_iter()
            .map(|m| m.held)
            .collect();
        assert_eq!(
            held,
            vec![
                Some(HoldReason::Limit),
                Some(HoldReason::Limit),
                Some(HoldReason::Stopped)
            ],
            "an earlier reason is kept"
        );
        store.hold_queue(&task, None).unwrap();
        assert!(
            store
                .queued(&task)
                .unwrap()
                .iter()
                .all(|m| m.held.is_none())
        );

        store.delete_queued(&[a.id, c.id]).unwrap();
        assert_eq!(store.queued(&task).unwrap().len(), 1);
        assert_eq!(store.queued_message(a.id).unwrap(), None);
    }

    #[test]
    fn queued_messages_need_their_task() {
        let store = store_with_task("t1");
        assert!(store.add_queued(&TaskId::new("nope"), "x", 1).is_err());
    }
}
