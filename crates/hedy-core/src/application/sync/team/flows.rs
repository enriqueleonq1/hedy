//! Flows and roles shared with a team (docs/decisions.md §49). They travel
//! as two more kinds of the team's encrypted repo, in the team layer. Only
//! admins share or remove them: unlike a note, they drive agents on every PC
//! (a role's instructions, a flow's test command). Every PC checks each
//! version against its writer's seat and refuses one that does not read, or
//! hides characters anywhere in it.

use super::*;
use crate::application::FlowScope;
use crate::domain::flow::{MAX_NODES, builtin_role, valid_name, valid_role_id};
use crate::domain::team::{TeamFlowBody, TeamRoleBody};

/// The team a project's flows can be shared with, and whether this PC may.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamShare {
    pub team: String,
    pub name: String,
    /// Only an admin shares or removes them.
    pub admin: bool,
}

/// A flow or role of the team, as this PC reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shared<T> {
    pub uid: String,
    /// Who shared it (the name an admin wrote for them, if any).
    pub author: String,
    pub body: T,
}

/// Whether a shared flow or role reads as one, with nothing hidden in it.
/// Every string of what it parses to is checked, keys too: an escaped
/// zero-width space or escape character in the JSON is hidden all the same.
pub(super) fn shared_config_ok(kind: ItemKind, version: &Version) -> bool {
    let Some(body) = version.body.as_deref() else {
        return true;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return false;
    };
    if !all_clean(&value) {
        return false;
    }
    match kind {
        ItemKind::Flow => serde_json::from_str::<TeamFlowBody>(body).is_ok_and(|b| {
            b.person.is_valid()
                && valid_name(&b.flow.name)
                && b.flow.nodes.len() <= MAX_NODES
                // Each PC runs a shared flow with its own accounts.
                && b.flow.seats().all(|s| s.account.is_none())
        }),
        ItemKind::Role => serde_json::from_str::<TeamRoleBody>(body).is_ok_and(|b| {
            b.person.is_valid()
                && valid_role_id(&b.role.id)
                && valid_name(&b.role.name)
                // Never in place of one of Hedy's: it would change the
                // user's own flows and the templates unseen.
                && builtin_role(&b.role.id).is_none()
        }),
        _ => false,
    }
}

/// No hidden character in any string or key of `value`.
pub(super) fn all_clean(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(s) => hidden_chars(s).is_empty(),
        serde_json::Value::Array(items) => items.iter().all(all_clean),
        serde_json::Value::Object(map) => map
            .iter()
            .all(|(k, v)| hidden_chars(k).is_empty() && all_clean(v)),
        _ => true,
    }
}

/// [`all_clean`] on something about to be shared: refused, showing where.
fn check_shareable<T: Serialize>(item: &T) -> AppResult<()> {
    let value = serde_json::to_value(item).unwrap_or(serde_json::Value::Null);
    if all_clean(&value) {
        Ok(())
    } else {
        Err(AppError::TeamHiddenText(reveal_hidden(&json(item))))
    }
}

impl Hedy {
    /// The team the project's flows can be shared with (its memory's team),
    /// if any.
    pub fn flow_sharing(&self, project: &Path) -> AppResult<Option<TeamShare>> {
        let project = self.project_at(project)?;
        let Some(space) = self.project_team(&project.path) else {
            return Ok(None);
        };
        let Space::Team(id) = &space else {
            return Ok(None);
        };
        let me = self.team_me(&space)?;
        Ok(Some(TeamShare {
            team: id.clone(),
            name: self.sync_setting(&space, TEAM_NAME)?.unwrap_or_default(),
            admin: me.seat.role == Role::Admin,
        }))
    }

    /// This PC in the project's team, when it is one of its admins.
    fn sharing_admin(&self, project: &Path) -> AppResult<(Space, Me)> {
        let project = self.project_at(project)?;
        let space = self
            .project_team(&project.path)
            .ok_or(AppError::TeamProjectNotLinked)?;
        let me = self.team_me(&space)?;
        if me.seat.role != Role::Admin {
            return Err(AppError::TeamNotAllowed);
        }
        Ok((space, me))
    }

    /// Shares one of the user's flows (`which`: its key or name) with the
    /// project's team, replacing the team's flow of the same name. It must
    /// run here first.
    pub fn team_share_flow(&self, project: &Path, which: &str) -> AppResult<String> {
        let entry = self.flow(Some(project), which)?;
        if !matches!(entry.scope, FlowScope::Project | FlowScope::Global) {
            return Err(AppError::FlowNotFound(which.to_owned()));
        }
        let problems = self.flow_problems(Some(project), &entry.flow)?;
        if !problems.is_empty() {
            return Err(AppError::InvalidFlow(
                problems.into_iter().map(|p| p.message).collect(),
            ));
        }
        // Seats name this PC's accounts: the team gets the steps, each PC
        // runs them with its own.
        let mut flow = entry.flow;
        for node in &mut flow.nodes {
            match &mut node.kind {
                crate::domain::flow::NodeKind::Role(seat) => seat.account = None,
                crate::domain::flow::NodeKind::Parallel { roles } => {
                    roles.iter_mut().for_each(|s| s.account = None)
                }
                _ => {}
            }
        }
        check_shareable(&flow)?;
        let (space, me) = self.sharing_admin(project)?;
        let _guard = self.sync_guard(&space)?;
        let uid = match self
            .team_flows_in(&space)?
            .into_iter()
            .find(|f| f.body.flow.name.trim().to_lowercase() == flow.name.trim().to_lowercase())
        {
            Some(same) => same.uid,
            None => item_uid(&me.seat.person, &self.sync_crypto.random_hex(8)?),
        };
        let name = flow.name.clone();
        let body = TeamFlowBody {
            flow,
            person: me.seat.person.clone(),
            author: me.name.clone(),
            created_at: self.now_secs(),
        };
        self.team_write(&me, ItemKind::Flow, &uid, Scope::Global, Some(json(&body)))?;
        Ok(name)
    }

    /// Stops sharing the team's flow named `name`.
    pub fn team_unshare_flow(&self, project: &Path, name: &str) -> AppResult<()> {
        let (space, me) = self.sharing_admin(project)?;
        let _guard = self.sync_guard(&space)?;
        let uid = self
            .team_flows_in(&space)?
            .into_iter()
            .find(|f| {
                f.uid == name
                    || f.body.flow.name.trim().to_lowercase() == name.trim().to_lowercase()
            })
            .map(|f| f.uid)
            .ok_or_else(|| AppError::FlowNotFound(name.to_owned()))?;
        self.team_write(&me, ItemKind::Flow, &uid, Scope::Global, None)
    }

    /// Shares one of the user's roles with the project's team, replacing
    /// the team's role with that id.
    pub fn team_share_role(&self, project: &Path, id: &str) -> AppResult<String> {
        let role = self
            .roles(Some(project))?
            .into_iter()
            .find(|r| r.role.id == id && matches!(r.scope, FlowScope::Project | FlowScope::Global))
            .ok_or_else(|| AppError::RoleNotFound(id.to_owned()))?
            .role;
        if builtin_role(&role.id).is_some() {
            let id = &role.id;
            return Err(AppError::InvalidRole(tr!(
                "«{id}» es un rol de Hedy: compártelo con otro id (`hedy roles new <id> --from {id}`)",
                "«{id}» is one of Hedy's roles: share it under another id (`hedy roles new <id> --from {id}`)"
            )));
        }
        check_shareable(&role)?;
        let (space, me) = self.sharing_admin(project)?;
        let _guard = self.sync_guard(&space)?;
        let uid = match self
            .team_roles_in(&space)?
            .into_iter()
            .find(|r| r.body.role.id == role.id)
        {
            Some(same) => same.uid,
            None => item_uid(&me.seat.person, &self.sync_crypto.random_hex(8)?),
        };
        let name = role.name.clone();
        let body = TeamRoleBody {
            role,
            person: me.seat.person.clone(),
            author: me.name.clone(),
            created_at: self.now_secs(),
        };
        self.team_write(&me, ItemKind::Role, &uid, Scope::Global, Some(json(&body)))?;
        Ok(name)
    }

    /// Stops sharing the team's role `id`.
    pub fn team_unshare_role(&self, project: &Path, id: &str) -> AppResult<()> {
        let (space, me) = self.sharing_admin(project)?;
        let _guard = self.sync_guard(&space)?;
        let uid = self
            .team_roles_in(&space)?
            .into_iter()
            .find(|r| r.body.role.id == id)
            .map(|r| r.uid)
            .ok_or_else(|| AppError::RoleNotFound(id.to_owned()))?;
        self.team_write(&me, ItemKind::Role, &uid, Scope::Global, None)
    }

    /// The flows the project's team shares (none for a project of no team).
    pub(crate) fn team_flows_for(&self, project: &Path) -> AppResult<Vec<Shared<TeamFlowBody>>> {
        match self.project_team(project) {
            Some(space) => self.team_flows_in(&space),
            None => Ok(Vec::new()),
        }
    }

    /// The roles the project's team shares.
    pub(crate) fn team_roles_for(&self, project: &Path) -> AppResult<Vec<Shared<TeamRoleBody>>> {
        match self.project_team(project) {
            Some(space) => self.team_roles_in(&space),
            None => Ok(Vec::new()),
        }
    }

    fn team_flows_in(&self, space: &Space) -> AppResult<Vec<Shared<TeamFlowBody>>> {
        self.shared_in(space, ItemKind::Flow, |b: &TeamFlowBody| {
            (&b.person, &b.author)
        })
    }

    fn team_roles_in(&self, space: &Space) -> AppResult<Vec<Shared<TeamRoleBody>>> {
        self.shared_in(space, ItemKind::Role, |b: &TeamRoleBody| {
            (&b.person, &b.author)
        })
    }

    fn shared_in<T: serde::de::DeserializeOwned>(
        &self,
        space: &Space,
        kind: ItemKind,
        byline: impl Fn(&T) -> (&PersonId, &String),
    ) -> AppResult<Vec<Shared<T>>> {
        let roster = self.roster(space)?;
        let mut items: Vec<Shared<T>> = self
            .store
            .sync_items(&space.id())?
            .into_iter()
            .filter(|c| c.kind == kind && shared_config_ok(kind, &c.version))
            .filter_map(|c| {
                let body: T = serde_json::from_str(c.version.body.as_deref()?).ok()?;
                let (person, author) = byline(&body);
                let author = roster
                    .get(person)
                    .cloned()
                    .unwrap_or_else(|| shown_name(author));
                Some(Shared {
                    uid: c.uid,
                    author,
                    body,
                })
            })
            .collect();
        items.sort_by(|a, b| a.uid.cmp(&b.uid));
        Ok(items)
    }
}
