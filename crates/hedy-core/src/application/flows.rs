//! The orchestrator's flows and roles (docs/decisions.md §42): Hedy's
//! templates and roles, and the ones the user saves for a project or for
//! every project. Nothing here runs a flow yet.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::ports::{FlowRecord, RoleRecord};
use super::{AppError, AppResult, Hedy};
use crate::domain::flow::{Flow, RoleDef, Template, builtin_role, builtin_roles};
use crate::domain::flow_text;
use crate::tr;

/// Where a flow or a role comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FlowScope {
    /// Shipped with Hedy (a template, a built-in role): never changes.
    Hedy,
    /// Saved for one project.
    Project,
    /// Saved for every project.
    Global,
}

/// A flow the user can pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowEntry {
    /// `classic`, `fix-loop`… for a template; the row id for a saved one.
    pub key: String,
    pub scope: FlowScope,
    /// One line of what it does (templates; empty for the user's).
    pub summary: String,
    pub flow: Flow,
}

/// A role of the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleEntry {
    pub scope: FlowScope,
    pub role: RoleDef,
}

/// Names match whatever their case, accents included («REVISIÓN»).
fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

impl Hedy {
    /// The project's own path when `dir` is in one (`None`: global only).
    fn flow_project(&self, project: Option<&Path>) -> AppResult<Option<crate::domain::Project>> {
        project.map(|p| self.project_at(p)).transpose()
    }

    /// Hedy's templates first (the classic one with the project's phases),
    /// then the project's flows and the global ones.
    pub fn flows(&self, project: Option<&Path>) -> AppResult<Vec<FlowEntry>> {
        let project = self.flow_project(project)?;
        let phases = project
            .as_ref()
            .map(|p| p.phases.clone())
            .unwrap_or_default();
        let mut entries: Vec<FlowEntry> = Template::ALL
            .into_iter()
            .map(|t| FlowEntry {
                key: t.key().to_owned(),
                scope: FlowScope::Hedy,
                summary: t.summary(),
                flow: t.flow(&phases),
            })
            .collect();
        let records = self.store.flow_records()?;
        let path = project.as_ref().map(|p| p.path.clone());
        for (scope, wanted) in [
            (FlowScope::Project, path.clone()),
            (FlowScope::Global, None),
        ] {
            if scope == FlowScope::Project && wanted.is_none() {
                continue;
            }
            entries.extend(
                records
                    .iter()
                    .filter(|r| r.project == wanted)
                    .map(|r| FlowEntry {
                        key: format!("#{}", r.id),
                        scope,
                        summary: String::new(),
                        flow: r.flow.clone(),
                    }),
            );
        }
        Ok(entries)
    }

    /// One flow by its key or its name (any case); the project's before the
    /// global ones with the same name.
    pub fn flow(&self, project: Option<&Path>, which: &str) -> AppResult<FlowEntry> {
        let wanted = which.trim();
        let entries = self.flows(project)?;
        entries
            .iter()
            .find(|e| e.key == wanted)
            // Then a name, the user's flows before Hedy's.
            .or_else(|| {
                entries
                    .iter()
                    .filter(|e| e.scope != FlowScope::Hedy)
                    .chain(entries.iter().filter(|e| e.scope == FlowScope::Hedy))
                    .find(|e| same_name(&e.flow.name, wanted))
            })
            .cloned()
            .ok_or_else(|| AppError::FlowNotFound(wanted.to_owned()))
    }

    /// A flow as text (`hedy flow show`), the form `save_flow_text` reads.
    pub fn flow_text(&self, project: Option<&Path>, which: &str) -> AppResult<String> {
        Ok(flow_text::to_text(&self.flow(project, which)?.flow))
    }

    /// Saves a flow written as text, for the project or for every project.
    /// It must be runnable: every problem is refused at once. One with the
    /// name of a saved flow in the same place replaces it.
    pub fn save_flow_text(&self, project: Option<&Path>, text: &str) -> AppResult<FlowEntry> {
        let flow =
            flow_text::from_text(text).map_err(|e| AppError::InvalidFlow(vec![e.to_string()]))?;
        self.save_flow(project, flow)
    }

    /// [`Hedy::save_flow_text`] for a flow already parsed (the canvas).
    pub fn save_flow(&self, project: Option<&Path>, flow: Flow) -> AppResult<FlowEntry> {
        // Hedy's templates keep their names: a flow of the user would hide one.
        if Template::ALL
            .into_iter()
            .any(|t| same_name(&t.name(), &flow.name) || same_name(t.key(), &flow.name))
        {
            let name = flow.name.trim();
            return Err(AppError::InvalidFlow(vec![tr!(
                "«{name}» es el nombre de una plantilla de Hedy: elige otro",
                "«{name}» is the name of one of Hedy's templates: choose another"
            )]));
        }
        let project = self.flow_project(project)?;
        let path = project.as_ref().map(|p| p.path.clone());
        let roles = self.role_lookup(path.as_deref())?;
        let problems: Vec<String> = flow
            .problems(&|id| roles(id))
            .iter()
            .map(|p| p.message())
            .collect();
        if !problems.is_empty() {
            return Err(AppError::InvalidFlow(problems));
        }
        let same = self
            .store
            .flow_records()?
            .into_iter()
            .find(|r| r.project == path && same_name(&r.flow.name, &flow.name));
        let record = FlowRecord {
            id: same.map_or(0, |r| r.id),
            project: path.clone(),
            flow,
            updated_at: self.now_secs(),
        };
        let id = self.store.save_flow(&record)?;
        Ok(FlowEntry {
            key: format!("#{id}"),
            scope: if path.is_some() {
                FlowScope::Project
            } else {
                FlowScope::Global
            },
            summary: String::new(),
            flow: record.flow,
        })
    }

    /// Deletes one of the user's flows; Hedy's templates stay.
    pub fn delete_flow(&self, project: Option<&Path>, which: &str) -> AppResult<()> {
        let entry = self.flow(project, which)?;
        match (
            entry.scope,
            entry.key.trim_start_matches('#').parse::<i64>(),
        ) {
            (FlowScope::Hedy, _) | (_, Err(_)) => Err(AppError::FlowNotFound(which.to_owned())),
            (_, Ok(id)) => Ok(self.store.delete_flow(id)?),
        }
    }

    /// Hedy's roles, then the project's and the global ones. A role of the
    /// user with a built-in id takes its place (a changed reviewer).
    pub fn roles(&self, project: Option<&Path>) -> AppResult<Vec<RoleEntry>> {
        let project = self.flow_project(project)?;
        let path: Option<PathBuf> = project.map(|p| p.path);
        let records: Vec<RoleRecord> = self.store.role_records()?;
        let mine = |scope: FlowScope, wanted: &Option<PathBuf>| -> Vec<RoleEntry> {
            records
                .iter()
                .filter(|r| &r.project == wanted)
                .map(|r| RoleEntry {
                    scope,
                    role: r.role.clone(),
                })
                .collect()
        };
        let mut user = if path.is_some() {
            mine(FlowScope::Project, &path)
        } else {
            Vec::new()
        };
        for global in mine(FlowScope::Global, &None) {
            if !user.iter().any(|u| u.role.id == global.role.id) {
                user.push(global);
            }
        }
        let mut entries: Vec<RoleEntry> = builtin_roles()
            .into_iter()
            .filter(|b| !user.iter().any(|u| u.role.id == b.id))
            .map(|role| RoleEntry {
                scope: FlowScope::Hedy,
                role,
            })
            .collect();
        entries.extend(user);
        Ok(entries)
    }

    /// How a flow of the project finds its roles by id.
    pub(super) fn role_lookup(
        &self,
        project: Option<&Path>,
    ) -> AppResult<impl Fn(&str) -> Option<RoleDef> + use<>> {
        let roles = match project {
            Some(path) => self.roles(Some(path))?,
            None => self.roles(None)?,
        };
        Ok(move |id: &str| {
            roles
                .iter()
                .find(|r| r.role.id == id)
                .map(|r| r.role.clone())
                .or_else(|| builtin_role(id))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::testing::Fixture;
    use crate::domain::flow::{Delivers, builtin_role};

    fn project(f: &Fixture) -> PathBuf {
        f.add_claude("Personal");
        let dir = f.dir("/code/tienda");
        f.hedy.use_account(&dir, "personal", None).unwrap();
        dir
    }

    #[test]
    fn templates_come_first_then_the_projects_and_the_global_flows() {
        let f = Fixture::new();
        let dir = project(&f);
        let keys: Vec<String> = f
            .hedy
            .flows(Some(&dir))
            .unwrap()
            .into_iter()
            .map(|e| e.key)
            .collect();
        assert_eq!(keys, ["classic", "fix-loop", "double-review"]);

        let text = f.hedy.flow_text(Some(&dir), "fix-loop").unwrap();
        let mine = text.replacen(&Template::FixLoop.name(), "Pago con revisión", 1);
        let saved = f.hedy.save_flow_text(Some(&dir), &mine).unwrap();
        assert_eq!(saved.scope, FlowScope::Project);
        let global = mine.replacen("Pago con revisión", "Para todos", 1);
        f.hedy.save_flow_text(None, &global).unwrap();

        let all = f.hedy.flows(Some(&dir)).unwrap();
        let names: Vec<(&str, FlowScope)> = all
            .iter()
            .map(|e| (e.flow.name.as_str(), e.scope))
            .collect();
        assert_eq!(
            names[3..],
            [
                ("Pago con revisión", FlowScope::Project),
                ("Para todos", FlowScope::Global)
            ]
        );
        // Outside a project, only the global ones.
        assert_eq!(f.hedy.flows(None).unwrap().len(), 4);
        // By name, any case; saving the same name again replaces it.
        assert_eq!(
            f.hedy.flow(Some(&dir), "pago con REVISIÓN").unwrap().key,
            saved.key
        );
        let again = f.hedy.save_flow_text(Some(&dir), &mine).unwrap();
        assert_eq!(again.key, saved.key);
        assert_eq!(f.hedy.flows(Some(&dir)).unwrap().len(), 5);

        f.hedy.delete_flow(Some(&dir), "Pago con revisión").unwrap();
        assert!(matches!(
            f.hedy.flow(Some(&dir), "Pago con revisión"),
            Err(AppError::FlowNotFound(_))
        ));
        assert!(matches!(
            f.hedy.delete_flow(Some(&dir), "classic"),
            Err(AppError::FlowNotFound(_))
        ));
    }

    #[test]
    fn an_unrunnable_flow_is_refused_with_every_reason() {
        let f = Fixture::new();
        let dir = project(&f);
        let text = f.hedy.flow_text(Some(&dir), "fix-loop").unwrap();
        let broken =
            text.replace("programmer", "nadie")
                .replacen(&Template::FixLoop.name(), "Mi flujo", 1);
        // Hedy's templates keep their names.
        assert!(matches!(
            f.hedy.save_flow_text(Some(&dir), &text),
            Err(AppError::InvalidFlow(_))
        ));
        let Err(AppError::InvalidFlow(problems)) = f.hedy.save_flow_text(Some(&dir), &broken)
        else {
            panic!("a flow with an unknown role was saved");
        };
        assert!(problems.iter().any(|p| p.contains("nadie")), "{problems:?}");
        assert!(matches!(
            f.hedy.save_flow_text(Some(&dir), "esto no es un flujo"),
            Err(AppError::InvalidFlow(_))
        ));
        assert_eq!(f.hedy.flows(Some(&dir)).unwrap().len(), 3);
    }

    #[test]
    fn a_users_role_takes_the_place_of_hedys_with_its_id() {
        use crate::application::ports::FlowRepository;
        let f = Fixture::new();
        let dir = project(&f);
        let roles = f.hedy.roles(Some(&dir)).unwrap();
        assert!(roles.iter().all(|r| r.scope == FlowScope::Hedy));
        let mut reviewer = builtin_role("reviewer").unwrap();
        reviewer.instructions = "Revisa también la accesibilidad.".into();
        reviewer.delivers = Delivers::Verdict;
        f.store
            .save_role(&RoleRecord {
                id: 0,
                project: Some(dir.clone()),
                role: reviewer.clone(),
                updated_at: 1,
            })
            .unwrap();
        let roles = f.hedy.roles(Some(&dir)).unwrap();
        let found: Vec<_> = roles.iter().filter(|r| r.role.id == "reviewer").collect();
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].scope, &found[0].role),
            (FlowScope::Project, &reviewer)
        );
        assert_eq!(
            f.hedy.roles(None).unwrap().len(),
            roles.len(),
            "global sees Hedy's"
        );
    }
}
