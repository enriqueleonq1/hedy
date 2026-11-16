//! Slash commands an agent's CLI offers (`/design`, `/review`, the user's
//! own commands and skills), as the CLI itself reports them (docs/decisions.md
//! §32). Hedy never reads an account folder to find them (rule 1).

use serde::{Deserialize, Serialize};

use super::{Account, Provider};

/// Where a command comes from, as far as the CLI says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandSource {
    /// Shipped with the CLI (Claude's built-ins and bundled skills).
    Builtin,
    /// The user's or the project's own command file (`.claude/commands`).
    Custom,
    /// A skill of the user or the project.
    Skill,
    /// Comes with an installed plugin (`plugin:command`).
    Plugin,
    /// The CLI did not say (ACP agents, MCP prompts).
    Unknown,
}

impl CommandSource {
    /// Written by the user or added by them, rather than shipped with the CLI.
    pub fn is_users(self) -> bool {
        matches!(self, Self::Custom | Self::Skill | Self::Plugin)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommand {
    /// Without the leading `/`: `design`, `studio:mockup`.
    pub name: String,
    pub description: Option<String>,
    pub source: CommandSource,
    /// What the command takes, e.g. `<name>` or `[low|high]`.
    pub argument_hint: Option<String>,
}

impl AgentCommand {
    pub fn new(name: impl Into<String>, source: CommandSource) -> Self {
        Self {
            name: name.into(),
            description: None,
            source,
            argument_hint: None,
        }
    }
}

/// The last list of commands an agent reported for an account (and, when
/// it was in a project, that project), and when (Unix seconds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownCommands {
    pub commands: Vec<AgentCommand>,
    pub seen_at: i64,
}

/// Commands that make no sense in a headless session Hedy drives: they log
/// in or out, change the CLI's settings or the session Hedy keeps (clear,
/// resume, model), or need an interactive terminal. Aliases included. They
/// are hidden from the lists; typed by hand they still go to the CLI, which
/// answers that they are not available or does what it does.
pub const HEADLESS_DENIED: &[&str] = &[
    "login",
    "logout",
    "config",
    "settings",
    "resume",
    "continue",
    "clear",
    "reset",
    "new",
    "exit",
    "quit",
    "vim",
    "terminal-setup",
    "theme",
    "model",
    "permissions",
];

/// Whether the account's agent tells Hedy its commands: Claude Code and
/// the ACP agents do; Codex's app server has no slash commands (its skills
/// go through `$name`), and an API key has no CLI at all.
pub fn reports_commands(account: &Account) -> bool {
    account.is_subscription() && account.provider != Provider::Codex
}

/// The commands worth offering: no denied or internal (`__name`) ones, no
/// empty or repeated names, a leading `/` dropped. Order is kept.
pub fn offerable(commands: Vec<AgentCommand>) -> Vec<AgentCommand> {
    let mut seen = std::collections::HashSet::new();
    commands
        .into_iter()
        .filter_map(|mut command| {
            command.name = command.name.trim().trim_start_matches('/').to_owned();
            let name = command.name.as_str();
            let usable = !name.is_empty()
                && !name.starts_with("__")
                && !name.contains(char::is_whitespace)
                && !HEADLESS_DENIED.contains(&name.to_lowercase().as_str());
            (usable && seen.insert(command.name.clone())).then_some(command)
        })
        .collect()
}

/// The command a message starts with: `/name` followed by a space or the
/// end, where the name has no `/` (so a path such as `/home/x` is not one).
pub fn slash_command(text: &str) -> Option<&str> {
    let rest = text.trim_start().strip_prefix('/')?;
    let name = rest.split(char::is_whitespace).next().unwrap_or_default();
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'));
    valid.then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_internal_and_repeated_commands_are_not_offered() {
        let commands = [
            "design",
            "/review",
            "login",
            "Clear",
            "__remote-workflow",
            "design",
            "",
            "two words",
            "usage",
            "studio:mockup",
        ]
        .map(|n| AgentCommand::new(n, CommandSource::Unknown))
        .to_vec();
        let names: Vec<String> = offerable(commands).into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["design", "review", "usage", "studio:mockup"]);
    }

    #[test]
    fn a_message_starts_with_a_command_only_when_it_looks_like_one() {
        assert_eq!(slash_command("/design login screen"), Some("design"));
        assert_eq!(slash_command("  /studio:mockup"), Some("studio:mockup"));
        assert_eq!(slash_command("/review\nmore"), Some("review"));
        assert_eq!(slash_command("/home/user/file.rs is broken"), None);
        assert_eq!(slash_command("/ nothing"), None);
        assert_eq!(slash_command("hello /design"), None);
        assert_eq!(slash_command("/"), None);
    }

    #[test]
    fn commands_serialize_in_camel_case() {
        let command = AgentCommand {
            argument_hint: Some("<screen>".into()),
            ..AgentCommand::new("design", CommandSource::Plugin)
        };
        let json = serde_json::to_value(&command).unwrap();
        assert_eq!(json["argumentHint"], "<screen>");
        assert_eq!(json["source"], "plugin");
        assert!(CommandSource::Skill.is_users() && !CommandSource::Builtin.is_users());
    }
}
