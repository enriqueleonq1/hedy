//! The orchestrator's flows and roles, each row a JSON document.

use std::path::PathBuf;

use rusqlite::params;

use super::{SqliteStore, json_error};
use crate::application::ports::{
    BoardRepository, FlowRecord, FlowRepository, PortResult, RoleRecord,
};
use crate::domain::TaskId;
use crate::domain::flow::BoardEntry;

impl FlowRepository for SqliteStore {
    fn flow_records(&self) -> PortResult<Vec<FlowRecord>> {
        self.with(|c| {
            c.prepare("SELECT id, project, data, updated_at FROM flows ORDER BY id")?
                .query_map([], |r| {
                    // A row that no longer reads (edited by hand) is left
                    // out: it must not take the templates and the rest with it.
                    let data: String = r.get(2)?;
                    let Ok(flow) = serde_json::from_str(&data) else {
                        return Ok(None);
                    };
                    Ok(Some(FlowRecord {
                        id: r.get(0)?,
                        project: r.get::<_, Option<String>>(1)?.map(PathBuf::from),
                        flow,
                        updated_at: r.get(3)?,
                    }))
                })?
                .filter_map(Result::transpose)
                .collect()
        })
    }

    fn save_flow(&self, record: &FlowRecord) -> PortResult<i64> {
        let data = serde_json::to_string(&record.flow).map_err(json_error)?;
        let project = record
            .project
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        self.with(|c| {
            if record.id == 0 {
                c.execute(
                    "INSERT INTO flows (project, name, data, updated_at) VALUES (?1, ?2, ?3, ?4)",
                    params![project, record.flow.name, data, record.updated_at],
                )?;
                Ok(c.last_insert_rowid())
            } else {
                c.execute(
                    "UPDATE flows SET project = ?2, name = ?3, data = ?4, updated_at = ?5
                     WHERE id = ?1",
                    params![
                        record.id,
                        project,
                        record.flow.name,
                        data,
                        record.updated_at
                    ],
                )?;
                Ok(record.id)
            }
        })
    }

    fn delete_flow(&self, id: i64) -> PortResult<()> {
        self.with(|c| c.execute("DELETE FROM flows WHERE id = ?1", [id]).map(drop))
    }

    fn role_records(&self) -> PortResult<Vec<RoleRecord>> {
        self.with(|c| {
            c.prepare("SELECT id, project, data, updated_at FROM flow_roles ORDER BY id")?
                .query_map([], |r| {
                    // A row that no longer reads (edited by hand) is left
                    // out: it must not take the templates and the rest with it.
                    let data: String = r.get(2)?;
                    let Ok(role) = serde_json::from_str(&data) else {
                        return Ok(None);
                    };
                    Ok(Some(RoleRecord {
                        id: r.get(0)?,
                        project: r.get::<_, Option<String>>(1)?.map(PathBuf::from),
                        role,
                        updated_at: r.get(3)?,
                    }))
                })?
                .filter_map(Result::transpose)
                .collect()
        })
    }

    fn save_role(&self, record: &RoleRecord) -> PortResult<i64> {
        let data = serde_json::to_string(&record.role).map_err(json_error)?;
        let project = record
            .project
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        self.with(|c| {
            if record.id == 0 {
                c.execute(
                    "INSERT INTO flow_roles (project, role, data, updated_at) VALUES (?1, ?2, ?3, ?4)",
                    params![project, record.role.id, data, record.updated_at],
                )?;
                Ok(c.last_insert_rowid())
            } else {
                c.execute(
                    "UPDATE flow_roles SET project = ?2, role = ?3, data = ?4, updated_at = ?5
                     WHERE id = ?1",
                    params![record.id, project, record.role.id, data, record.updated_at],
                )?;
                Ok(record.id)
            }
        })
    }

    fn delete_role(&self, id: i64) -> PortResult<()> {
        self.with(|c| {
            c.execute("DELETE FROM flow_roles WHERE id = ?1", [id])
                .map(drop)
        })
    }
}

impl BoardRepository for SqliteStore {
    fn board(&self, task: &TaskId) -> PortResult<Vec<BoardEntry>> {
        self.with(|c| {
            c.prepare(
                "SELECT section, role, round, text, by_user FROM flow_board WHERE task = ?1 ORDER BY id",
            )?
            .query_map([task.as_str()], |r| {
                Ok(BoardEntry {
                    section: r.get(0)?,
                    role: r.get(1)?,
                    round: r.get(2)?,
                    text: r.get(3)?,
                    by_user: r.get(4)?,
                })
            })?
            .collect()
        })
    }

    fn post_board(&self, task: &TaskId, e: &BoardEntry) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO flow_board (task, section, role, round, text, by_user)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![task.as_str(), e.section, e.role, e.round, e.text, e.by_user],
            )
            .map(drop)
        })
    }

    fn clear_board(&self, task: &TaskId) -> PortResult<()> {
        self.with(|c| {
            c.execute("DELETE FROM flow_board WHERE task = ?1", [task.as_str()])
                .map(drop)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::PhasePlan;
    use crate::domain::flow::{Template, builtin_role};

    #[test]
    fn flows_and_roles_round_trip_per_project_or_global() {
        let store = SqliteStore::in_memory().unwrap();
        let flow = Template::Classic.flow(&PhasePlan::new());
        let mut record = FlowRecord {
            id: 0,
            project: Some("/code/tienda".into()),
            flow: flow.clone(),
            updated_at: 10,
        };
        record.id = store.save_flow(&record).unwrap();
        let global = FlowRecord {
            id: 0,
            project: None,
            ..record.clone()
        };
        let global_id = store.save_flow(&global).unwrap();
        assert_eq!(
            store.flow_records().unwrap(),
            vec![
                record.clone(),
                FlowRecord {
                    id: global_id,
                    ..global
                }
            ]
        );
        record.flow.name = "Pago con revisión".into();
        record.updated_at = 20;
        store.save_flow(&record).unwrap();
        store.delete_flow(global_id).unwrap();
        assert_eq!(store.flow_records().unwrap(), vec![record]);

        let mut role = RoleRecord {
            id: 0,
            project: None,
            role: builtin_role("reviewer").unwrap(),
            updated_at: 5,
        };
        role.role.id = "accesibilidad".into();
        role.id = store.save_role(&role).unwrap();
        assert_eq!(store.role_records().unwrap(), vec![role.clone()]);
        store.delete_role(role.id).unwrap();
        assert!(store.role_records().unwrap().is_empty());
    }

    #[test]
    fn a_row_that_no_longer_reads_is_left_out() {
        let store = SqliteStore::in_memory().unwrap();
        let good = FlowRecord {
            id: 0,
            project: None,
            flow: Template::Classic.flow(&PhasePlan::new()),
            updated_at: 1,
        };
        let id = store.save_flow(&good).unwrap();
        store
            .with(|c| {
                c.execute(
                    "INSERT INTO flows (project, name, data, updated_at) VALUES (NULL, 'x', '{roto', 2)",
                    [],
                )
                .map(drop)
            })
            .unwrap();
        assert_eq!(
            store.flow_records().unwrap(),
            vec![FlowRecord { id, ..good }]
        );
    }

    #[test]
    fn a_runs_board_is_kept_per_task_in_order_and_cleared() {
        let store = SqliteStore::in_memory().unwrap();
        let (a, b) = (TaskId::new("t1"), TaskId::new("t2"));
        let entry = |section: &str, text: &str| BoardEntry {
            section: section.into(),
            role: "Arquitecto".into(),
            round: 1,
            text: text.into(),
            by_user: false,
        };
        store.post_board(&a, &entry("plan", "1. Pagos")).unwrap();
        store.post_board(&b, &entry("plan", "otra tarea")).unwrap();
        let feedback = BoardEntry {
            by_user: true,
            ..entry("feedback", "en el servidor")
        };
        store.post_board(&a, &feedback).unwrap();
        assert_eq!(
            store.board(&a).unwrap(),
            vec![entry("plan", "1. Pagos"), feedback]
        );
        store.clear_board(&a).unwrap();
        assert!(store.board(&a).unwrap().is_empty());
        assert_eq!(store.board(&b).unwrap().len(), 1);
    }
}
