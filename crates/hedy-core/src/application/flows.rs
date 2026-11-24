//! The orchestrator's flows and roles (docs/decisions.md §42): Hedy's
//! templates and roles, and the ones the user saves for a project or for
//! every project. Nothing here runs a flow yet.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::ports::{FlowRecord, RoleRecord};
use super::{AppError, AppResult, Hedy};
use crate::domain::flow::{Check, Flow, NodeKind, RoleDef, Template, builtin_role, builtin_roles};
use crate::domain::policy;
use crate::domain::{flow_text, role_text};
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
    /// Shared by the project's team (docs/decisions.md §49): read-only here.
    Team,
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
    /// Who shared it (a team's flow).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// Its test commands this PC has not allowed yet (a team's flow): it
    /// does not start until they are (docs/decisions.md §49).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unapproved: Vec<String>,
}

/// Something that keeps a flow from running, for the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowIssue {
    /// The step to mark, when it is about one.
    pub node: Option<String>,
    pub message: String,
}

/// A team flow's key: this, then its uid in the team.
pub const TEAM_KEY: &str = "team:";

/// Where the test commands this PC allowed for flows shared by a team are
/// kept (exact strings: a changed command asks again).
const APPROVED_COMMANDS: &str = "flows.approved_commands";

/// A role of the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleEntry {
    pub scope: FlowScope,
    pub role: RoleDef,
}

fn taken(name: &str) -> AppError {
    let name = name.trim();
    AppError::InvalidFlow(vec![tr!(
        "ya hay un flujo «{name}»: elige otro nombre",
        "there is already a flow «{name}»: choose another name"
    )])
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
                by: None,
                unapproved: Vec::new(),
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
                        by: None,
                        unapproved: Vec::new(),
                    }),
            );
        }
        // Then the team's, as shared (the user's own come first by name).
        if let Some(path) = &path {
            for shared in self.team_flows_for(path)? {
                let unapproved = self.unapproved_commands(&shared.body.flow)?;
                entries.push(FlowEntry {
                    key: format!("{TEAM_KEY}{}", shared.uid),
                    scope: FlowScope::Team,
                    summary: String::new(),
                    flow: shared.body.flow,
                    by: Some(shared.author),
                    unapproved,
                });
            }
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
            // Then a name: the user's flows, Hedy's, then the team's (a team
            // flow never takes a template's name).
            .or_else(|| {
                let of = |scope: FlowScope| entries.iter().filter(move |e| e.scope == scope);
                of(FlowScope::Project)
                    .chain(of(FlowScope::Global))
                    .chain(of(FlowScope::Hedy))
                    .chain(of(FlowScope::Team))
                    .find(|e| same_name(&e.flow.name, wanted))
            })
            .cloned()
            .ok_or_else(|| AppError::FlowNotFound(wanted.to_owned()))
    }

    /// The flows a user picks from: `flows` without the retired templates.
    pub fn offered_flows(&self, project: Option<&Path>) -> AppResult<Vec<FlowEntry>> {
        let mut entries = self.flows(project)?;
        entries.retain(|e| {
            e.scope != FlowScope::Hedy || !Template::from_key(&e.key).is_some_and(Template::retired)
        });
        Ok(entries)
    }

    /// The flow a project's new «Cambiar código» tasks start with
    /// (docs/decisions.md §56). `None`: one AI, also when the chosen flow
    /// is gone since.
    pub fn default_flow(&self, project: &Path) -> AppResult<Option<FlowEntry>> {
        let project = self.project_at(project)?;
        let Some(key) = project.default_flow else {
            return Ok(None);
        };
        Ok(self
            .flows(Some(&project.path))?
            .into_iter()
            .find(|e| e.key == key))
    }

    /// Sets it, by key or name; `None` goes back to one AI.
    pub fn set_default_flow(
        &self,
        project: &Path,
        which: Option<&str>,
    ) -> AppResult<Option<FlowEntry>> {
        let mut project = self.project_at(project)?;
        let entry = which
            .map(|which| self.flow(Some(&project.path), which))
            .transpose()?;
        project.default_flow = entry.as_ref().map(|e| e.key.clone());
        self.store.save_project(&project)?;
        Ok(entry)
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

    /// [`Hedy::save_flow_text`] for a flow already parsed: one with the same
    /// name in the same place is replaced.
    pub fn save_flow(&self, project: Option<&Path>, flow: Flow) -> AppResult<FlowEntry> {
        self.store_flow(project, None, flow)
    }

    /// The editor's save: `key` (`#<id>`) updates that flow, renamed or not,
    /// where it was saved; `None` saves a new one for `project`. Either way a
    /// name another flow of the same place has is refused.
    pub fn save_flow_as(
        &self,
        project: Option<&Path>,
        key: Option<&str>,
        flow: Flow,
    ) -> AppResult<FlowEntry> {
        self.store_flow(project, Some(key), flow)
    }

    /// `edit`: `None` replaces by name; `Some(key)` as in [`Hedy::save_flow_as`].
    fn store_flow(
        &self,
        project: Option<&Path>,
        edit: Option<Option<&str>>,
        flow: Flow,
    ) -> AppResult<FlowEntry> {
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
        let records = self.store.flow_records()?;
        let editing = match edit.flatten() {
            Some(key) => {
                let id = key.strip_prefix('#').and_then(|id| id.parse::<i64>().ok());
                // Only a flow this project can see: its own or a global one.
                let record = records
                    .iter()
                    .find(|r| Some(r.id) == id && (r.project.is_none() || r.project == path));
                Some(record.ok_or_else(|| AppError::FlowNotFound(key.to_owned()))?)
            }
            None => None,
        };
        // A saved flow stays where it was (a global one stays global), and is
        // checked with the roles and accounts of that place.
        let place = editing.map_or(path, |r| r.project.clone());
        let issues = self.flow_issues(place.as_deref(), &flow)?;
        if !issues.is_empty() {
            return Err(AppError::InvalidFlow(
                issues.into_iter().map(|i| i.message).collect(),
            ));
        }
        let same = records
            .iter()
            .find(|r| r.project == place && same_name(&r.flow.name, &flow.name));
        let id = match (edit, editing, same) {
            // By name: the one with that name.
            (None, _, same) => same.map_or(0, |r| r.id),
            (Some(_), Some(mine), Some(other)) if other.id != mine.id => {
                return Err(taken(&flow.name));
            }
            (Some(_), None, Some(_)) => return Err(taken(&flow.name)),
            (Some(_), editing, _) => editing.map_or(0, |r| r.id),
        };
        let record = FlowRecord {
            id,
            project: place,
            flow,
            updated_at: self.now_secs(),
        };
        let id = self.store.save_flow(&record)?;
        Ok(FlowEntry {
            key: format!("#{id}"),
            scope: if record.project.is_some() {
                FlowScope::Project
            } else {
                FlowScope::Global
            },
            summary: String::new(),
            flow: record.flow,
            by: None,
            unapproved: Vec::new(),
        })
    }

    /// Why a flow being edited could not run yet (empty: it can), for the
    /// editor to show as it changes.
    pub fn flow_problems(&self, project: Option<&Path>, flow: &Flow) -> AppResult<Vec<FlowIssue>> {
        let project = self.flow_project(project)?;
        self.flow_issues(project.as_ref().map(|p| p.path.as_path()), flow)
    }

    /// The flow's problems with the roles of `place` (a project, or none for
    /// a global flow), then each seat whose account may not work there: a
    /// protected project's account is not offered elsewhere (rule 4).
    fn flow_issues(&self, place: Option<&Path>, flow: &Flow) -> AppResult<Vec<FlowIssue>> {
        let roles = self.role_lookup(place)?;
        let name = |id: &str| roles(id).map(|r| r.name);
        let mut issues: Vec<FlowIssue> = flow
            .problems(&|id| roles(id))
            .iter()
            .map(|p| {
                let node = p.node().map(str::to_owned);
                let step = node.as_deref().and_then(|id| flow.node(id));
                let message = match step {
                    Some(step) => p.naming(&step.label(&name)).message(),
                    None => p.message(),
                };
                FlowIssue { node, message }
            })
            .collect();
        let projects = self.projects()?;
        let project = place.and_then(|p| projects.iter().find(|x| x.path == p));
        for node in &flow.nodes {
            let seats: &[crate::domain::flow::Seat] = match &node.kind {
                NodeKind::Role(seat) => std::slice::from_ref(seat),
                NodeKind::Parallel { roles } => roles,
                _ => &[],
            };
            for account in seats.iter().filter_map(|s| s.account.as_ref()) {
                let problem = match self.account(account.as_str()) {
                    Err(e) => Some(e.to_string()),
                    Ok(_) => policy::check_launch(project, account, &projects)
                        .err()
                        .map(|e| e.to_string()),
                };
                if let Some(message) = problem {
                    issues.push(FlowIssue {
                        node: Some(node.id.clone()),
                        message,
                    });
                }
            }
        }
        Ok(issues)
    }

    /// Deletes one of the user's flows; Hedy's templates stay.
    pub fn delete_flow(&self, project: Option<&Path>, which: &str) -> AppResult<()> {
        let entry = self.flow(project, which)?;
        match (
            entry.scope,
            entry.key.trim_start_matches('#').parse::<i64>(),
        ) {
            (FlowScope::Hedy | FlowScope::Team, _) | (_, Err(_)) => {
                Err(AppError::FlowNotFound(which.to_owned()))
            }
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
        // The team's roles, under the user's own with the same id.
        if let Some(path) = &path {
            for shared in self.team_roles_for(path)? {
                if !user.iter().any(|u| u.role.id == shared.body.role.id)
                    && builtin_role(&shared.body.role.id).is_none()
                {
                    user.push(RoleEntry {
                        scope: FlowScope::Team,
                        role: shared.body.role,
                    });
                }
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

    /// The flow's own test commands this PC has not allowed.
    pub(crate) fn unapproved_commands(&self, flow: &Flow) -> AppResult<Vec<String>> {
        let approved = self.approved_commands()?;
        let mut pending: Vec<String> = flow
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Condition(Check::Tests { command: Some(c) }) => Some(c.clone()),
                _ => None,
            })
            .filter(|c| !approved.contains(c))
            .collect();
        pending.dedup();
        Ok(pending)
    }

    fn approved_commands(&self) -> AppResult<Vec<String>> {
        Ok(self
            .store
            .get_setting(APPROVED_COMMANDS)?
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default())
    }

    /// Allows a test command of a team's flow on this PC (the user's
    /// decision, never an agent's).
    pub fn approve_flow_command(&self, command: &str) -> AppResult<()> {
        let mut approved = self.approved_commands()?;
        if !approved.iter().any(|c| c == command) {
            approved.push(command.to_owned());
        }
        let text = serde_json::to_string(&approved).unwrap_or_default();
        Ok(self.store.set_setting(APPROVED_COMMANDS, Some(&text))?)
    }

    /// A role of the library as text (`hedy roles show`).
    pub fn role_text(&self, project: Option<&Path>, id: &str) -> AppResult<String> {
        let role = self
            .roles(project)?
            .into_iter()
            .find(|r| r.role.id == id)
            .ok_or_else(|| AppError::RoleNotFound(id.to_owned()))?;
        Ok(role_text::to_text(&role.role))
    }

    /// Saves a role written as text, for the project (`global`: for every
    /// project). One with the same id in the same place is replaced; one with
    /// a built-in id takes Hedy's place there.
    pub fn save_role_text(
        &self,
        project: Option<&Path>,
        text: &str,
        global: bool,
    ) -> AppResult<RoleEntry> {
        let role = role_text::from_text(text).map_err(|e| AppError::InvalidRole(e.to_string()))?;
        let project = if global {
            None
        } else {
            self.flow_project(project)?.map(|p| p.path)
        };
        let same = self
            .store
            .role_records()?
            .into_iter()
            .find(|r| r.project == project && r.role.id == role.id);
        let record = RoleRecord {
            id: same.map_or(0, |r| r.id),
            project: project.clone(),
            role,
            updated_at: self.now_secs(),
        };
        self.store.save_role(&record)?;
        Ok(RoleEntry {
            scope: if project.is_some() {
                FlowScope::Project
            } else {
                FlowScope::Global
            },
            role: record.role,
        })
    }

    /// Removes a role of the user (the project's first, else the global one);
    /// a built-in one it replaced comes back.
    pub fn delete_role(&self, project: Option<&Path>, id: &str) -> AppResult<()> {
        let project = self.flow_project(project)?.map(|p| p.path);
        let records = self.store.role_records()?;
        let record = records
            .iter()
            .find(|r| r.role.id == id && r.project.is_some() && r.project == project)
            .or_else(|| {
                records
                    .iter()
                    .find(|r| r.role.id == id && r.project.is_none())
            })
            .ok_or_else(|| AppError::RoleNotFound(id.to_owned()))?;
        Ok(self.store.delete_role(record.id)?)
    }

    /// The editor's text save of flow `key` (`hedy flow edit`): it keeps the
    /// flow, renamed or not.
    pub fn save_flow_text_as(
        &self,
        project: Option<&Path>,
        key: &str,
        text: &str,
    ) -> AppResult<FlowEntry> {
        let flow =
            flow_text::from_text(text).map_err(|e| AppError::InvalidFlow(vec![e.to_string()]))?;
        // The text keeps no positions: the canvas lays it out again.
        self.save_flow_as(project, Some(key), flow)
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
    use crate::domain::flow::{Delivers, When, builtin_role};

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
        assert_eq!(keys, ["essential", "approval", "focused-review"]);
        let offered: Vec<_> = f
            .hedy
            .offered_flows(Some(&dir))
            .unwrap()
            .into_iter()
            .filter(|e| e.scope == FlowScope::Hedy)
            .map(|e| e.key)
            .collect();
        assert_eq!(
            offered,
            ["essential", "focused-review"],
            "«Con tu aprobación» is retired"
        );

        let text = f.hedy.flow_text(Some(&dir), "focused-review").unwrap();
        let mine = text.replacen(&Template::FocusedReview.name(), "Pago con revisión", 1);
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
            f.hedy.delete_flow(Some(&dir), "approval"),
            Err(AppError::FlowNotFound(_))
        ));
    }

    #[test]
    fn an_unrunnable_flow_is_refused_with_every_reason() {
        let f = Fixture::new();
        let dir = project(&f);
        let text = f.hedy.flow_text(Some(&dir), "focused-review").unwrap();
        let broken = text.replace("test-first", "nadie").replacen(
            &Template::FocusedReview.name(),
            "Mi flujo",
            1,
        );
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
    fn the_editor_saves_by_key_renames_in_place_and_refuses_a_taken_name() {
        let f = Fixture::new();
        let dir = project(&f);
        let mut flow = f.hedy.flow(Some(&dir), "focused-review").unwrap().flow;
        flow.name = "Pagos".into();
        let first = f.hedy.save_flow_as(Some(&dir), None, flow.clone()).unwrap();
        // A new one with the same name is refused, not replaced.
        assert!(matches!(
            f.hedy.save_flow_as(Some(&dir), None, flow.clone()),
            Err(AppError::InvalidFlow(_))
        ));
        flow.name = "Cobros".into();
        let renamed = f
            .hedy
            .save_flow_as(Some(&dir), Some(&first.key), flow.clone())
            .unwrap();
        assert_eq!(renamed.key, first.key);
        let names: Vec<String> = f
            .hedy
            .flows(Some(&dir))
            .unwrap()
            .into_iter()
            .filter(|e| e.scope != FlowScope::Hedy)
            .map(|e| e.flow.name)
            .collect();
        assert_eq!(names, ["Cobros"]);
        // Renaming onto another flow's name is refused.
        flow.name = "Otro".into();
        let other = f.hedy.save_flow_as(Some(&dir), None, flow.clone()).unwrap();
        flow.name = "cobros".into();
        assert!(matches!(
            f.hedy
                .save_flow_as(Some(&dir), Some(&other.key), flow.clone()),
            Err(AppError::InvalidFlow(_))
        ));
        // A key that is not one of its flows.
        assert!(matches!(
            f.hedy.save_flow_as(Some(&dir), Some("#999"), flow),
            Err(AppError::FlowNotFound(_))
        ));
    }

    #[test]
    fn a_seat_on_another_protected_projects_account_is_a_problem_on_its_step() {
        use crate::domain::flow::NodeKind;
        let f = Fixture::new();
        let dir = project(&f);
        f.add_claude("Trabajo");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "trabajo", Some(true))
            .unwrap();
        let mut flow = f.hedy.flow(Some(&dir), "focused-review").unwrap().flow;
        flow.name = "Prestada".into();
        for node in &mut flow.nodes {
            if let NodeKind::Role(seat) = &mut node.kind {
                seat.account = Some(crate::domain::AccountId::new("trabajo").unwrap());
            }
        }
        let issues = f.hedy.flow_problems(Some(&dir), &flow).unwrap();
        assert_eq!(
            issues
                .iter()
                .filter_map(|i| i.node.as_deref())
                .collect::<Vec<_>>(),
            ["n2", "n4"]
        );
        assert!(matches!(
            f.hedy.save_flow_as(Some(&dir), None, flow),
            Err(AppError::InvalidFlow(_))
        ));
    }

    #[test]
    fn a_role_written_as_text_joins_the_library_and_leaves_it() {
        let f = Fixture::new();
        let dir = project(&f);
        let text = "id: accesibilidad\nname: Accesibilidad\ndelivers: verdict\ncan: []\nsees: all\ninstructions: |\n  Revisa el teclado.\n";
        let entry = f.hedy.save_role_text(Some(&dir), text, false).unwrap();
        assert_eq!(entry.scope, FlowScope::Project);
        assert!(
            f.hedy
                .role_text(Some(&dir), "accesibilidad")
                .unwrap()
                .contains("Revisa el teclado.")
        );
        // Changing the reviewer here takes Hedy's place; removing it brings it back.
        let reviewer = f
            .hedy
            .role_text(Some(&dir), "reviewer")
            .unwrap()
            .replace("name: Revisor", "name: Revisor estricto")
            .replace("name: Reviewer", "name: Revisor estricto");
        f.hedy.save_role_text(Some(&dir), &reviewer, false).unwrap();
        let named = |id: &str| {
            f.hedy
                .roles(Some(&dir))
                .unwrap()
                .into_iter()
                .find(|r| r.role.id == id)
                .unwrap()
        };
        assert_eq!(named("reviewer").role.name, "Revisor estricto");
        f.hedy.delete_role(Some(&dir), "reviewer").unwrap();
        assert_eq!(named("reviewer").scope, FlowScope::Hedy);
        assert!(matches!(
            f.hedy.delete_role(Some(&dir), "reviewer"),
            Err(AppError::RoleNotFound(_))
        ));
        assert!(matches!(
            f.hedy.save_role_text(Some(&dir), "id: x\n", false),
            Err(AppError::InvalidRole(_))
        ));
    }

    #[test]
    fn the_editor_gets_each_problem_on_its_step_by_name() {
        let f = Fixture::new();
        let dir = project(&f);
        let mut flow = f.hedy.flow(Some(&dir), "focused-review").unwrap().flow;
        assert!(f.hedy.flow_problems(Some(&dir), &flow).unwrap().is_empty());
        // The review's verdict loses its way back to the programmer.
        flow.edges
            .retain(|e| !(e.from == "n5" && e.when == When::Fail));
        let issues = f.hedy.flow_problems(Some(&dir), &flow).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].node.as_deref(), Some("n5"));
        assert!(
            issues[0].message.contains("«¿Lo aprueban?»"),
            "{}",
            issues[0].message
        );
        assert!(!issues[0].message.contains("n5"));
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

    #[test]
    fn a_project_starts_its_code_tasks_with_its_default_flow() {
        let f = Fixture::new();
        let dir = project(&f);
        assert_eq!(f.hedy.default_flow(&dir).unwrap(), None);
        let chosen = f.hedy.set_default_flow(&dir, Some("Esencial")).unwrap();
        assert_eq!(chosen.map(|e| e.key), Some("essential".into()));
        assert_eq!(
            f.hedy.default_flow(&dir).unwrap().map(|e| e.key),
            Some("essential".into())
        );
        assert!(matches!(
            f.hedy.set_default_flow(&dir, Some("No existe")),
            Err(AppError::FlowNotFound(_))
        ));
        f.hedy.set_default_flow(&dir, None).unwrap();
        assert_eq!(f.hedy.default_flow(&dir).unwrap(), None);
    }
}
