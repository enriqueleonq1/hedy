//! The orchestrator's flows and roles, each row a JSON document.

use std::path::PathBuf;

use rusqlite::params;

use super::{SqliteStore, invalid, json_error};
use crate::application::ports::{FlowRecord, FlowRepository, PortResult, RoleRecord};

impl FlowRepository for SqliteStore {
    fn flow_records(&self) -> PortResult<Vec<FlowRecord>> {
        self.with(|c| {
            c.prepare("SELECT id, project, data, updated_at FROM flows ORDER BY id")?
                .query_map([], |r| {
                    Ok(FlowRecord {
                        id: r.get(0)?,
                        project: r.get::<_, Option<String>>(1)?.map(PathBuf::from),
                        flow: serde_json::from_str(&r.get::<_, String>(2)?).map_err(invalid(2))?,
                        updated_at: r.get(3)?,
                    })
                })?
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
                    Ok(RoleRecord {
                        id: r.get(0)?,
                        project: r.get::<_, Option<String>>(1)?.map(PathBuf::from),
                        role: serde_json::from_str(&r.get::<_, String>(2)?).map_err(invalid(2))?,
                        updated_at: r.get(3)?,
                    })
                })?
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
}
