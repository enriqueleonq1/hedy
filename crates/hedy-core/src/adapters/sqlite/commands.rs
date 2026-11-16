//! The slash commands each account's CLI last reported.

use std::path::{Path, PathBuf};

use rusqlite::params;

use super::SqliteStore;
use crate::application::ports::{CommandRepository, PortResult};
use crate::domain::{AccountId, AgentCommand, KnownCommands};

impl CommandRepository for SqliteStore {
    fn set_agent_commands(
        &self,
        account: &AccountId,
        project: Option<&Path>,
        known: &KnownCommands,
    ) -> PortResult<()> {
        let project = project.map_or_else(String::new, |p| p.to_string_lossy().into_owned());
        let commands = serde_json::to_string(&known.commands).unwrap_or_else(|_| "[]".into());
        self.with(|c| {
            c.execute(
                "INSERT INTO agent_commands (account_id, project, commands, seen_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (account_id, project)
                 DO UPDATE SET commands = excluded.commands, seen_at = excluded.seen_at",
                params![account.as_str(), project, commands, known.seen_at],
            )
            .map(drop)
        })
    }

    fn agent_commands(
        &self,
        account: &AccountId,
    ) -> PortResult<Vec<(Option<PathBuf>, KnownCommands)>> {
        let rows: Vec<(String, String, i64)> = self.with(|c| {
            c.prepare(
                "SELECT project, commands, seen_at FROM agent_commands
                 WHERE account_id = ?1 ORDER BY seen_at DESC, project",
            )?
            .query_map([account.as_str()], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect()
        })?;
        // A list Hedy cannot read any more is as good as unknown.
        Ok(rows
            .into_iter()
            .filter_map(|(project, commands, seen_at)| {
                let commands: Vec<AgentCommand> = serde_json::from_str(&commands).ok()?;
                let project = (!project.is_empty()).then(|| PathBuf::from(project));
                Some((project, KnownCommands { commands, seen_at }))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::CommandSource;

    fn known(names: &[&str], seen_at: i64) -> KnownCommands {
        KnownCommands {
            commands: names
                .iter()
                .map(|n| AgentCommand::new(*n, CommandSource::Skill))
                .collect(),
            seen_at,
        }
    }

    #[test]
    fn lists_are_kept_per_account_and_project_newest_first() {
        let store = SqliteStore::in_memory().unwrap();
        let personal = AccountId::new("personal").unwrap();
        let work = AccountId::new("work").unwrap();
        store
            .set_agent_commands(&personal, None, &known(&["design"], 10))
            .unwrap();
        store
            .set_agent_commands(
                &personal,
                Some(Path::new("/code/x")),
                &known(&["deploy"], 20),
            )
            .unwrap();
        store
            .set_agent_commands(&work, None, &known(&["review"], 5))
            .unwrap();
        // Replaced, not added.
        store
            .set_agent_commands(&personal, None, &known(&["design", "brand"], 30))
            .unwrap();

        let lists = store.agent_commands(&personal).unwrap();
        assert_eq!(lists.len(), 2);
        assert_eq!(lists[0], (None, known(&["design", "brand"], 30)));
        assert_eq!(lists[1].0, Some(PathBuf::from("/code/x")));
        assert_eq!(store.agent_commands(&work).unwrap().len(), 1);
        assert!(
            store
                .agent_commands(&AccountId::new("other").unwrap())
                .unwrap()
                .is_empty()
        );
    }
}
