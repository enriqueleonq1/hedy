//! Prompt templates and the changes to the built-in examples.

use std::path::PathBuf;

use rusqlite::params;

use super::SqliteStore;
use crate::application::ports::{PortResult, TemplateRecord, TemplateRepository};

impl TemplateRepository for SqliteStore {
    fn template_records(&self) -> PortResult<Vec<TemplateRecord>> {
        self.with(|c| {
            c.prepare(
                "SELECT id, project, name, text, builtin, deleted FROM prompt_templates ORDER BY id",
            )?
            .query_map([], |r| {
                Ok(TemplateRecord {
                    id: r.get(0)?,
                    project: r.get::<_, Option<String>>(1)?.map(PathBuf::from),
                    name: r.get(2)?,
                    text: r.get(3)?,
                    builtin: r.get(4)?,
                    deleted: r.get(5)?,
                })
            })?
            .collect()
        })
    }

    fn save_template(&self, t: &TemplateRecord) -> PortResult<i64> {
        let project = t.project.as_ref().map(|p| p.to_string_lossy().into_owned());
        self.with(|c| {
            if t.id == 0 {
                c.execute(
                    "INSERT INTO prompt_templates (project, name, text, builtin, deleted)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![project, t.name, t.text, t.builtin, t.deleted],
                )?;
                Ok(c.last_insert_rowid())
            } else {
                c.execute(
                    "UPDATE prompt_templates
                     SET project = ?2, name = ?3, text = ?4, builtin = ?5, deleted = ?6
                     WHERE id = ?1",
                    params![t.id, project, t.name, t.text, t.builtin, t.deleted],
                )?;
                Ok(t.id)
            }
        })
    }

    fn delete_template(&self, id: i64) -> PortResult<()> {
        self.with(|c| {
            c.execute("DELETE FROM prompt_templates WHERE id = ?1", [id])
                .map(drop)
        })
    }

    fn reset_builtin_templates(&self) -> PortResult<()> {
        self.with(|c| {
            c.execute("DELETE FROM prompt_templates WHERE builtin IS NOT NULL", [])
                .map(drop)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(project: Option<&str>, name: &str, builtin: Option<&str>) -> TemplateRecord {
        TemplateRecord {
            id: 0,
            project: project.map(PathBuf::from),
            name: name.into(),
            text: format!("{name} {{archivo}}"),
            builtin: builtin.map(str::to_owned),
            deleted: false,
        }
    }

    #[test]
    fn templates_round_trip_and_builtins_reset() {
        let store = SqliteStore::in_memory().unwrap();
        let global = store.save_template(&record(None, "Global", None)).unwrap();
        let local = store
            .save_template(&record(Some("/code/x"), "Local", None))
            .unwrap();
        let mut edited = record(None, "Pruebas", Some("tests"));
        edited.id = store.save_template(&edited).unwrap();
        edited.deleted = true;
        store.save_template(&edited).unwrap();

        let records = store.template_records().unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].id, global);
        assert_eq!(records[1].project, Some(PathBuf::from("/code/x")));
        assert_eq!(records[1].text, "Local {archivo}");
        assert!(records[2].deleted);

        store.reset_builtin_templates().unwrap();
        store.delete_template(local).unwrap();
        let names: Vec<_> = store
            .template_records()
            .unwrap()
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, vec!["Global"]);
    }
}
