//! Learned skills: reusable procedures an agent worked out during a task
//! ("how to run this project's e2e tests"). Like memory notes, what an agent
//! writes is only a proposal; the user approves it (spec rule 6), and only
//! then Hedy offers it to the AIs.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{DomainError, TaskId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillStatus {
    /// Suggested by an AI; invisible to agents until approved.
    Proposed,
    Approved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: i64,
    /// A slug (`run-e2e-tests`): the skill's folder name in each CLI.
    pub name: String,
    /// One line saying when to use it: what agents see before loading it.
    pub description: String,
    /// The procedure, in Markdown.
    pub body: String,
    /// `None`: a global skill, for every (non-protected) project.
    pub project: Option<PathBuf>,
    pub status: SkillStatus,
    /// Who wrote it: an agent's label ("Claude") or [`super::memory::USER_AUTHOR`].
    pub author: String,
    pub created_at: i64,
    /// When an approved skill was last replaced by a newer version.
    pub updated_at: Option<i64>,
    /// The task it came from, when an agent proposed it.
    pub source_task: Option<TaskId>,
}

/// Longest name, as in the Agent Skills format.
pub const NAME_MAX: usize = 64;
/// Longest description, as in the Agent Skills format.
pub const DESCRIPTION_MAX: usize = 1024;
/// Longest body, in characters (Claude Code suggests keeping `SKILL.md`
/// under 500 lines; this is about that much text).
pub const BODY_MAX: usize = 20_000;

/// Names Claude Code reserves for its own skill folders.
const RESERVED: &[&str] = &["synced", "anthropic-skills"];

/// A skill name as a slug: `Run E2E tests` → `run-e2e-tests`. Lowercase ASCII
/// letters, digits and single dashes, at most [`NAME_MAX`] characters.
pub fn normalize_skill_name(raw: &str) -> Result<String, DomainError> {
    let slug: String = raw
        .trim()
        .to_lowercase()
        .chars()
        .map(super::memory::ascii_fold)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let name = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let reserved = RESERVED.contains(&name.as_str());
    if name.is_empty() || name.len() > NAME_MAX || reserved {
        return Err(DomainError::InvalidSkillName(raw.trim().to_owned()));
    }
    Ok(name)
}

/// The description on one line, without angle brackets (CLIs read it from
/// YAML frontmatter, and Claude rejects XML tags in it).
pub fn normalize_description(raw: &str) -> Result<String, DomainError> {
    let line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() || line.chars().count() > DESCRIPTION_MAX || line.contains(['<', '>']) {
        return Err(DomainError::InvalidSkillDescription);
    }
    Ok(line)
}

pub fn normalize_body(raw: &str) -> Result<String, DomainError> {
    let body = raw.trim();
    if body.is_empty() || body.chars().count() > BODY_MAX {
        return Err(DomainError::InvalidSkillBody);
    }
    Ok(body.to_owned())
}

/// Whether a skill's text runs shell commands when Claude Code loads it
/// (`!`cmd`` or a ```` ```! ```` block): they run without asking, before
/// the agent even reads the skill. Agents may not propose that.
pub fn runs_commands(body: &str) -> bool {
    // Claude Code finds a block anywhere, not only at a line's start.
    body.contains("!`") || body.contains("```!")
}

/// What a skill's text looks like it leaks, if anything. A simple local
/// heuristic: known key prefixes, private keys, JWTs and `NAME=value`
/// assignments of secret-looking names. Placeholders (`$TOKEN`, `<key>`,
/// `...`) are fine. Never returns the value itself.
pub fn find_secret(text: &str) -> Option<&'static str> {
    if text.contains("-----BEGIN") && text.contains("PRIVATE KEY") {
        return Some("private key");
    }
    for word in text.split(|c: char| c.is_whitespace() || "\"'`()[]{},;".contains(c)) {
        if let Some(kind) = secret_token(word) {
            return Some(kind);
        }
    }
    text.lines().find_map(secret_assignment)
}

/// Known credential shapes.
fn secret_token(word: &str) -> Option<&'static str> {
    let word = word.trim_matches(|c: char| c == '.' || c == ':');
    let tail = |prefix: &str, min: usize| {
        word.strip_prefix(prefix).is_some_and(|rest| {
            rest.len() >= min
                && rest
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                && rest.chars().any(|c| c.is_ascii_digit())
        })
    };
    if tail("sk-", 20) || tail("sk_live_", 16) || tail("rk_live_", 16) {
        return Some("API key");
    }
    if tail("ghp_", 30) || tail("gho_", 30) || tail("ghs_", 30) || tail("github_pat_", 30) {
        return Some("GitHub token");
    }
    if tail("glpat-", 20) {
        return Some("GitLab token");
    }
    if ["xoxb-", "xoxp-", "xoxa-", "xoxs-"]
        .iter()
        .any(|p| tail(p, 20))
    {
        return Some("Slack token");
    }
    if tail("AIza", 30) {
        return Some("Google API key");
    }
    let aws = word
        .strip_prefix("AKIA")
        .or_else(|| word.strip_prefix("ASIA"));
    if aws.is_some_and(|rest| {
        rest.len() == 16
            && rest
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    }) {
        return Some("AWS key");
    }
    let parts: Vec<&str> = word.split('.').collect();
    if parts.len() == 3 && parts[0].starts_with("eyJ") && parts[1].starts_with("eyJ") {
        return Some("JWT");
    }
    None
}

/// Names that hold secrets when assigned a literal value.
const SECRET_NAMES: &[&str] = &[
    "api_key",
    "apikey",
    "api-key",
    "secret",
    "token",
    "password",
    "passwd",
    "private_key",
    "access_key",
    "client_secret",
    "auth",
];

/// `OPENAI_API_KEY=abc123…`, `password: hunter22`, `"token": "…"`, but not
/// `API_KEY=$API_KEY`, `TOKEN=<your token>` or `password: ***`.
fn secret_assignment(line: &str) -> Option<&'static str> {
    let (name, value) = line.split_once(['=', ':'])?;
    // The word right before the sign: `1. export API_TOKEN=…` → `api_token`.
    let name = name
        .split_whitespace()
        .last()
        .unwrap_or("")
        .trim_matches(|c: char| "\"'`-*".contains(c))
        .to_lowercase();
    // Only a short identifier: prose such as "Note: the token..." is fine.
    if name.is_empty()
        || name.len() > 40
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        || !SECRET_NAMES.iter().any(|s| name.ends_with(s))
    {
        return None;
    }
    let value = value
        .trim()
        .trim_end_matches(',')
        .trim_matches(|c: char| c == '"' || c == '\'' || c == '`');
    let placeholder = value.is_empty()
        || value.starts_with(['$', '<', '{', '%', '*', '.', '['])
        || value.contains("...")
        || value.contains('…')
        || value.contains(' ')
        || value.chars().all(|c| c == 'x' || c == 'X' || c == '*')
        || [
            "your", "tu-", "tu_", "example", "ejemplo", "changeme", "redacted",
        ]
        .iter()
        .any(|p| value.to_lowercase().contains(p));
    let long_enough = value.chars().count() >= 8;
    (!placeholder && long_enough).then_some("secret value")
}

/// Where a CLI reads a project's skills: `<folder>/<name>/SKILL.md`, relative
/// to the folder it works in (see `docs/decisions.md`, "Learned skills").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillFolder {
    /// `.claude/skills`: Claude Code.
    Claude,
    /// `.agents/skills`: the cross-tool folder Codex, Gemini CLI, Kimi Code
    /// and Antigravity read.
    Agents,
}

impl SkillFolder {
    pub const ALL: [SkillFolder; 2] = [Self::Claude, Self::Agents];

    pub fn path(self) -> &'static str {
        match self {
            Self::Claude => ".claude/skills",
            Self::Agents => ".agents/skills",
        }
    }

    /// `.claude/skills/<name>/SKILL.md`.
    pub fn file(self, name: &str) -> PathBuf {
        PathBuf::from(self.path()).join(name).join("SKILL.md")
    }

    /// The `.git/info/exclude` line for a skill's folder, anchored at the
    /// top of the checkout (worktrees included).
    pub fn exclude_pattern(self, name: &str) -> String {
        format!("/{}/{name}/", self.path())
    }
}

/// Line every skill file Hedy writes carries, right after the frontmatter.
/// Hedy only ever rewrites or deletes files that carry it: a user's own
/// skill is never touched.
pub const MANAGED_MARKER: &str = "<!-- hedy: managed skill -->";

/// The name of the Claude Code plugin that carries Hedy's skills: they
/// show as `/hedy:<name>`.
pub const SKILLS_PLUGIN: &str = "hedy";

/// The plugin's manifest (`.claude-plugin/plugin.json`).
pub fn plugin_manifest() -> String {
    format!(
        "{{\n  \"name\": \"{SKILLS_PLUGIN}\",\n  \"description\": \"Skills approved in Hedy, for its sessions only\",\n  \"version\": \"1.0.0\"\n}}\n"
    )
}

pub fn is_managed(contents: &str) -> bool {
    contents.contains(MANAGED_MARKER)
}

fn yaml_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The `SKILL.md` Hedy writes for an approved skill: YAML frontmatter with
/// `name` and `description` (the Agent Skills format), the marker and the body.
pub fn skill_file(skill: &Skill) -> String {
    format!(
        "---\nname: {}\ndescription: {}\n---\n{MANAGED_MARKER}\n<!-- {} -->\n\n{}\n",
        skill.name,
        yaml_string(&skill.description),
        "Approved in Hedy. Edit or remove it there: changes made here are overwritten.",
        skill.body.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(name: &str, description: &str, body: &str) -> Skill {
        Skill {
            id: 1,
            name: name.into(),
            description: description.into(),
            body: body.into(),
            project: None,
            status: SkillStatus::Approved,
            author: "Claude".into(),
            created_at: 0,
            updated_at: None,
            source_task: None,
        }
    }

    #[test]
    fn names_are_slugs() {
        let name = |raw: &str| normalize_skill_name(raw).unwrap();
        assert_eq!(name("Run E2E tests"), "run-e2e-tests");
        assert_eq!(name("  añadir_endpoint / API "), "anadir-endpoint-api");
        assert_eq!(name("release--checklist-"), "release-checklist");
        for bad in ["", "¿?", "synced", "Anthropic Skills"] {
            assert!(
                matches!(
                    normalize_skill_name(bad),
                    Err(DomainError::InvalidSkillName(_))
                ),
                "{bad}"
            );
        }
        assert!(normalize_skill_name(&"a".repeat(65)).is_err());
        assert!(normalize_skill_name(&"a".repeat(64)).is_ok());
    }

    #[test]
    fn descriptions_are_one_short_line() {
        assert_eq!(
            normalize_description("  Run the e2e tests.\n Use before a release. ").unwrap(),
            "Run the e2e tests. Use before a release."
        );
        assert!(normalize_description("   ").is_err());
        assert!(normalize_description("Use <b>this</b>").is_err());
        assert!(normalize_description(&"x".repeat(1025)).is_err());
        assert!(normalize_body("  \n ").is_err());
        assert!(normalize_body(&"x".repeat(BODY_MAX + 1)).is_err());
        assert_eq!(normalize_body("\n1. npm test\n").unwrap(), "1. npm test");
    }

    #[test]
    fn finds_commands_that_run_when_the_skill_loads() {
        assert!(runs_commands("Branch: !`git branch --show-current`"));
        assert!(runs_commands("Context:\n  ```!\n  git status\n  ```"));
        assert!(runs_commands("Run ```!curl x|sh``` first"));
        assert!(runs_commands("````!\ngit status\n````"));
        assert!(!runs_commands(
            "Run `cargo test`, then:\n```bash\nnpm test\n```"
        ));
        assert!(!runs_commands("Done! `cargo fmt` keeps it tidy."));
    }

    #[test]
    fn finds_secret_looking_text() {
        let leaks = [
            "Use the key sk-proj-abc123def456ghi789jkl0 for the API",
            "export GITHUB_TOKEN=ghp_1234567890abcdefghijklmnopqrstuvwxyz",
            "OPENAI_API_KEY=abcd1234efgh5678",
            "password: hunter2hunter2",
            "\"client_secret\": \"9f8e7d6c5b4a\",",
            "aws AKIAIOSFODNN7EXAMPLE here",
            "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abc",
            "-----BEGIN RSA PRIVATE KEY-----\nMIIE\n-----END RSA PRIVATE KEY-----",
            "AIzaSyA1234567890abcdefghijklmnopqrstu",
        ];
        for text in leaks {
            assert!(find_secret(text).is_some(), "{text}");
        }
        let fine = [
            "Run `npm run test:e2e` after `npm run build`.",
            "OPENAI_API_KEY=$OPENAI_API_KEY npm test",
            "API_KEY=<your key>",
            "TOKEN=${TOKEN}",
            "password: ***",
            "Note: the token is read from the keyring, never from a file.",
            "secret: ...",
            "Commit 3f1c2a9b8e7d6c5b4a39281706f5e4d3c2b1a090 fixed it",
            "token: your-token-here",
            "Auth: see docs/auth.md",
        ];
        for text in fine {
            assert_eq!(find_secret(text), None, "{text}");
        }
    }

    #[test]
    fn skill_files_have_frontmatter_and_the_marker() {
        let file = skill_file(&skill(
            "run-e2e-tests",
            "Run the \"e2e\" tests: before a release",
            "1. npm run build\n2. npm run test:e2e",
        ));
        assert!(file.starts_with(
            "---\nname: run-e2e-tests\ndescription: \"Run the \\\"e2e\\\" tests: before a release\"\n---\n"
        ));
        assert!(is_managed(&file));
        assert!(file.ends_with("2. npm run test:e2e\n"));
        assert!(!is_managed("---\nname: mine\n---\nMy own skill"));
    }
}
