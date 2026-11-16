//! The slash commands of each account (`/design`, `/review`, the user's own
//! commands and skills): only what the agent's CLI reports when a session
//! starts, never read from the account folder (rule 1). Kept per account and
//! per project, since a project can add its own (docs/decisions.md §32).

use std::path::Path;

use super::{AppResult, Hedy};
use crate::domain::command::offerable;
use crate::domain::{Account, AccountId, AgentCommand, CommandSource, KnownCommands, TaskId};

impl Hedy {
    /// Stores the commands a session of `account` reported, for the task's
    /// project (none for a conversation, whose folder is empty).
    pub(super) fn remember_commands(
        &self,
        id: &TaskId,
        account: &AccountId,
        commands: Vec<AgentCommand>,
    ) -> AppResult<()> {
        let task = self.task(id)?;
        let project = (!task.conversation).then_some(task.project.as_path());
        let known = KnownCommands {
            commands: offerable(commands),
            seen_at: self.now_secs(),
        };
        Ok(self.store.set_agent_commands(account, project, &known)?)
    }

    /// The commands known for `account` in `project` (`None`: outside any
    /// project). Without a list from that project, the one seen outside any
    /// project (the account's own commands), else, from the newest list seen
    /// in another project, only what belongs to the account rather than to
    /// that project (built-ins and plugins). `None` until the account answers
    /// once in Hedy.
    pub fn account_commands(
        &self,
        account: &str,
        project: Option<&Path>,
    ) -> AppResult<Option<KnownCommands>> {
        let account = self.account(account)?;
        let lists = self.store.agent_commands(&account.id)?;
        let pick = |wanted: Option<&Path>| {
            lists
                .iter()
                .find(|(p, _)| p.as_deref() == wanted)
                .map(|(_, k)| k.clone())
        };
        Ok(project
            .and_then(|p| pick(Some(p)))
            .or_else(|| pick(None))
            .or_else(|| lists.first().map(|(_, k)| account_wide(k))))
    }

    /// For `hedy commands`: the account given, else the project's, else the
    /// default one, with what is known of its commands in the project
    /// containing `dir`. Only lists: no protection rule is involved.
    pub fn commands_in(
        &self,
        dir: &Path,
        account: Option<&str>,
    ) -> AppResult<(Account, Option<KnownCommands>)> {
        let project = self.project_for(dir)?;
        let id = match account {
            Some(id) => self.account(id)?.id,
            None => project
                .as_ref()
                .and_then(|p| p.account.clone())
                .or(self.default_account_id()?)
                .ok_or(super::AppError::NoAccountResolved)?,
        };
        let account = self.account(id.as_str())?;
        let known =
            self.account_commands(id.as_str(), project.as_ref().map(|p| p.path.as_path()))?;
        Ok((account, known))
    }
}

/// The part of a list seen in some project that holds in any other: the
/// CLI's built-ins and the account's plugins. Custom commands and skills may
/// be that project's own (`.claude/commands`, `.claude/skills`).
fn account_wide(known: &KnownCommands) -> KnownCommands {
    KnownCommands {
        commands: known
            .commands
            .iter()
            .filter(|c| matches!(c.source, CommandSource::Builtin | CommandSource::Plugin))
            .cloned()
            .collect(),
        seen_at: known.seen_at,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::application::testing::Fixture;
    use crate::application::{NewConversation, NewTask, Setting};
    use crate::domain::{AgentEvent, CommandSource, TaskMode};

    fn commands(names: &[&str]) -> AgentEvent {
        AgentEvent::Commands {
            commands: names
                .iter()
                .map(|n| AgentCommand::new(*n, CommandSource::Skill))
                .collect(),
        }
    }

    #[test]
    fn another_projects_list_lends_only_built_ins_and_plugins() {
        let known = KnownCommands {
            commands: vec![
                AgentCommand::new("review", CommandSource::Builtin),
                AgentCommand::new("studio:mockup", CommandSource::Plugin),
                AgentCommand::new("deploy", CommandSource::Custom),
                AgentCommand::new("pricing", CommandSource::Skill),
                AgentCommand::new("mcp__x__y", CommandSource::Unknown),
            ],
            seen_at: 7,
        };
        let lent = super::account_wide(&known);
        assert_eq!(names(Some(lent.clone())), ["review", "studio:mockup"]);
        assert_eq!(lent.seen_at, 7);
    }

    fn names(known: Option<KnownCommands>) -> Vec<String> {
        known
            .map(|k| k.commands.into_iter().map(|c| c.name).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_conversation_keeps_the_accounts_commands_without_the_deny_list() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.hedy
            .set_setting(Setting::DefaultAccount, Some("personal"))
            .unwrap();
        assert_eq!(f.hedy.account_commands("personal", None).unwrap(), None);

        let task = f
            .hedy
            .start_conversation(NewConversation {
                prompt: "/design login".into(),
                mode: TaskMode::Ask,
                account: None,
                model: None,
                images: Vec::new(),
            })
            .unwrap();
        // Sent verbatim.
        assert_eq!(f.agents.last_request().prompt, "/design login");
        f.agents
            .emit(commands(&["design", "login", "clear", "review"]));

        let known = f.hedy.account_commands("personal", None).unwrap();
        assert_eq!(names(known.clone()), ["design", "review"]);
        assert!(known.unwrap().seen_at > 0);
        // Never part of the task's history.
        let history = f.hedy.task_events(&task.id).unwrap();
        assert!(
            !history
                .iter()
                .any(|e| matches!(e, AgentEvent::Commands { .. }))
        );
        // Any project without its own list falls back to the account's.
        assert_eq!(
            names(
                f.hedy
                    .account_commands("personal", Some(Path::new("/code/x")))
                    .unwrap()
            ),
            ["design", "review"]
        );
        assert!(f.hedy.account_commands("missing", None).is_err());
    }

    #[test]
    fn a_project_task_keeps_its_own_list() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        let project = f.hedy.project_at(&f.dir("/code/agenda")).unwrap();
        f.hedy
            .start_task(NewTask {
                project: project.path.clone(),
                prompt: "/deploy staging".into(),
                mode: TaskMode::Ask,
                model: None,
                plan_first: false,
                images: Vec::new(),
            })
            .unwrap();
        f.agents.emit(commands(&["deploy", "design"]));

        assert_eq!(
            names(
                f.hedy
                    .account_commands("personal", Some(&project.path))
                    .unwrap()
            ),
            ["deploy", "design"]
        );
        // Outside the project, only what is not that project's own: custom
        // commands and skills may live in its `.claude` folder.
        assert_eq!(
            names(f.hedy.account_commands("personal", None).unwrap()),
            Vec::<&str>::new()
        );
        let (account, known) = f.hedy.commands_in(&project.path, None).unwrap();
        assert_eq!(account.id.as_str(), "personal");
        assert_eq!(names(known), ["deploy", "design"]);
        assert!(matches!(
            f.hedy.commands_in(&PathBuf::from("/elsewhere"), None),
            Err(crate::application::AppError::NoAccountResolved)
        ));
    }
}
