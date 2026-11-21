//! Shipping a task's work (commit, push, pull request) and the main folder's
//! git state. Only the pure parts live here: reading `git status
//! --porcelain=v2`, the remote's host, why a push or a pull failed, the
//! commit suggestions and the pull request's body.

use serde::{Deserialize, Serialize};

use super::diff::{FileDiff, LineKind};
use super::permissions::is_secret_path;
use super::rule_check::shown_path;
use crate::tr;

/// The main folder's git state, as `git status --porcelain=v2 --branch` tells it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitFolder {
    /// `None` on a detached HEAD.
    pub branch: Option<String>,
    /// "origin/main"; `None` when the branch tracks nothing.
    pub upstream: Option<String>,
    /// Commits the branch has that its upstream does not.
    pub ahead: u32,
    /// Commits the upstream has that the branch does not.
    pub behind: u32,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    /// Files with merge conflicts.
    pub conflicted: u32,
    /// Files with any uncommitted change; a file both staged and changed
    /// again counts once.
    pub changed: u32,
}

/// Parses `git status --porcelain=v2 --branch` (without `-z`).
pub fn parse_status_v2(out: &str) -> GitFolder {
    let mut folder = GitFolder::default();

    for line in out.lines() {
        if let Some(header) = line.strip_prefix("# ") {
            if let Some(head) = header.strip_prefix("branch.head ") {
                folder.branch = (head != "(detached)").then(|| head.to_owned());
            } else if let Some(upstream) = header.strip_prefix("branch.upstream ") {
                folder.upstream = Some(upstream.to_owned());
            } else if let Some(ab) = header.strip_prefix("branch.ab ") {
                for part in ab.split_whitespace() {
                    if let Some(n) = part.strip_prefix('+') {
                        folder.ahead = n.parse().unwrap_or(0);
                    } else if let Some(n) = part.strip_prefix('-') {
                        folder.behind = n.parse().unwrap_or(0);
                    }
                }
            }
            continue;
        }
        let mut parts = line.splitn(3, ' ');
        match (parts.next(), parts.next()) {
            (Some("1" | "2"), Some(xy)) => {
                let mut flags = xy.chars();
                let (x, y) = (flags.next().unwrap_or('.'), flags.next().unwrap_or('.'));
                folder.staged += u32::from(x != '.');
                folder.unstaged += u32::from(y != '.');
                folder.changed += 1;
            }
            (Some("u"), _) => {
                folder.conflicted += 1;
                folder.changed += 1;
            }
            (Some("?"), _) => {
                folder.untracked += 1;
                folder.changed += 1;
            }
            _ => {}
        }
    }
    folder
}

/// The host of a remote URL ("github.com"), never its user or password:
/// `https://user:token@github.com/a/b.git`, `git@github.com:a/b.git` and
/// `ssh://git@github.com:22/a/b` all give "github.com". `None` for local
/// paths.
pub fn remote_host(url: &str) -> Option<String> {
    let url = url.trim();
    let authority = if let Some((scheme, rest)) = url.split_once("://") {
        if scheme == "file" {
            return None;
        }
        rest.split('/').next().unwrap_or("")
    } else if url.starts_with('/') || url.starts_with('.') || url.starts_with('~') {
        return None;
    } else {
        // scp-like: [user@]host:path
        let (head, _) = url.split_once(':')?;
        if head.contains('/') {
            return None;
        }
        head
    };
    let host = authority.rsplit('@').next().unwrap_or("");
    let host = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// The remote to publish to: the one named `origin`, else the only one.
pub fn pick_remote(remotes: &[String]) -> Option<String> {
    remotes
        .iter()
        .find(|r| *r == "origin")
        .or_else(|| (remotes.len() == 1).then(|| &remotes[0]))
        .cloned()
}

/// Whether GitHub CLI can open pull requests for this folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GhState {
    /// `gh` is not installed.
    Missing,
    /// Installed, without a working login for the remote's host.
    LoggedOut,
    Ready,
}

/// Why a push failed, from git's own message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PushProblem {
    /// The remote refused the user's credentials (or there were none).
    Auth,
    /// The remote has commits the branch lacks (non-fast-forward).
    Rejected,
    /// The remote protects the branch.
    Protected,
    /// The remote could not be reached.
    Offline,
    Other,
}

/// Why a fast-forward pull failed, from git's own message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PullProblem {
    /// The branch and its upstream both have new commits.
    Diverged,
    /// The branch tracks no remote branch.
    NoUpstream,
    /// Local changes would be overwritten.
    LocalChanges,
    Auth,
    Offline,
    Other,
}

fn any(message: &str, needles: &[&str]) -> bool {
    let lower = message.to_ascii_lowercase();
    needles.iter().any(|n| lower.contains(n))
}

const AUTH: &[&str] = &[
    "authentication failed",
    "could not read username",
    "could not read password",
    "permission denied",
    "terminal prompts disabled",
    "invalid username or password",
    "could not read from remote repository",
    "the requested url returned error: 403",
    "the requested url returned error: 401",
];

const OFFLINE: &[&str] = &[
    "could not resolve host",
    "could not resolve hostname",
    "failed to connect",
    "network is unreachable",
    "connection timed out",
    "operation timed out",
    "connection refused",
];

impl PushProblem {
    pub fn from_git(message: &str) -> Self {
        if any(message, &["protected branch", "gh006"]) {
            Self::Protected
        } else if any(
            message,
            &["non-fast-forward", "fetch first", "updates were rejected"],
        ) {
            Self::Rejected
        } else if any(message, OFFLINE) {
            Self::Offline
        } else if any(message, AUTH) {
            Self::Auth
        } else {
            Self::Other
        }
    }
}

impl PullProblem {
    pub fn from_git(message: &str) -> Self {
        if any(
            message,
            &[
                "not possible to fast-forward",
                "diverging branches",
                "diverged",
            ],
        ) {
            Self::Diverged
        } else if any(message, &["no tracking information", "no upstream"]) {
            Self::NoUpstream
        } else if any(
            message,
            &["would be overwritten", "commit your changes or stash them"],
        ) {
            Self::LocalChanges
        } else if any(message, OFFLINE) {
            Self::Offline
        } else if any(message, AUTH) {
            Self::Auth
        } else {
            Self::Other
        }
    }
}

/// How "Aceptar" lands the work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AcceptMode {
    /// Commit and merge into the base branch in the main folder (as always).
    #[default]
    Merge,
    /// Commit on the task's own branch and keep it, to publish it and open a
    /// pull request from it. The main folder is not touched.
    Branch,
}

/// Where an accepted task's work went, and what Hedy published of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shipment {
    /// The branch that carries the work: the base branch after a merge, or
    /// the task's own branch when it was kept.
    pub branch: String,
    /// Kept on the task's own branch instead of merged.
    pub kept: bool,
    /// The commit message (its first line is the pull request's title).
    pub message: String,
    /// What changed, for the pull request's body.
    #[serde(default)]
    pub files: Vec<ShippedFile>,
    /// The remote branch Hedy pushed it to ("origin/main").
    #[serde(default)]
    pub pushed_to: Option<String>,
    #[serde(default)]
    pub pull_request: Option<PullRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShippedFile {
    pub path: String,
    pub additions: u32,
    pub deletions: u32,
}

impl From<&FileDiff> for ShippedFile {
    fn from(f: &FileDiff) -> Self {
        Self {
            path: f.path.clone(),
            additions: f.additions,
            deletions: f.deletions,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    /// Always `https://`.
    pub url: String,
    pub number: Option<u32>,
    pub draft: bool,
}

impl PullRequest {
    /// From what `gh pr create` prints: the first `https://…/pull/N` link,
    /// also when it says one already exists. `None` without such a link.
    pub fn from_gh_output(out: &str, draft: bool) -> Option<Self> {
        let url = out
            .split_whitespace()
            .find(|w| w.starts_with("https://") && w.contains("/pull/"))?
            .trim_end_matches(['.', ',', ')'])
            .to_owned();
        let number = url.rsplit("/pull/").next()?.split(['/', '#', '?']).next()?;
        Some(Self {
            number: number.parse().ok(),
            url,
            draft,
        })
    }
}

/// The first line of a commit message.
pub fn commit_subject(message: &str) -> &str {
    message
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
}

/// Whether most recent commit subjects follow Conventional Commits
/// (`type(scope): message`). Needs at least two of them.
pub fn uses_conventional_commits<'a>(subjects: impl IntoIterator<Item = &'a str>) -> bool {
    let subjects: Vec<&str> = subjects
        .into_iter()
        .filter(|s| !s.starts_with("Merge "))
        .collect();
    let conventional = subjects.iter().filter(|s| is_conventional(s)).count();
    conventional >= 2 && conventional * 2 >= subjects.len()
}

fn is_conventional(subject: &str) -> bool {
    let Some((head, rest)) = subject.split_once(": ") else {
        return false;
    };
    let head = head.trim_end_matches('!');
    let kind = head.split('(').next().unwrap_or("");
    !rest.trim().is_empty()
        && !kind.is_empty()
        && kind.chars().all(|c| c.is_ascii_lowercase())
        && (!head.contains('(') || head.ends_with(')'))
}

/// A Conventional Commit subject from a task's title: the type guessed from
/// its first words ("Arregla…" → `fix`), the title with a lowercase start.
pub fn conventional_subject(title: &str) -> String {
    let title = title.trim().trim_end_matches('…').trim_end_matches('.');
    if is_conventional(title) {
        return title.to_owned();
    }
    let lower = title.to_lowercase();
    let first = lower
        .split(|c: char| !c.is_alphanumeric())
        .find(|w| !w.is_empty())
        .unwrap_or("");
    let kind = match first {
        "fix" | "arregla" | "arreglar" | "corrige" | "corregir" | "repara" | "soluciona" => "fix",
        "test" | "tests" | "prueba" | "pruebas" => "test",
        "refactor" | "refactoriza" | "reorganiza" | "simplifica" | "limpia" | "clean" => "refactor",
        "docs" | "doc" | "document" | "documenta" | "documentar" => "docs",
        "style" | "formatea" | "format" => "style",
        "chore" | "actualiza" | "update" | "bump" => "chore",
        _ if lower.contains("test") => "test",
        _ => "feat",
    };
    let mut chars = title.chars();
    let subject: String = match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    };
    format!("{kind}: {subject}")
}

/// The changes as a commit message request shows them: each file's path
/// (hidden characters shown) and counts, then its hunks' added lines. A
/// removed line is not sent (it may be a secret being taken out), nor a
/// protected file's contents (`.env`, keys).
pub fn diff_for_prompt(files: &[FileDiff], max_chars: usize) -> String {
    let mut out = String::new();
    for file in files {
        out.push_str(&format!(
            "--- {} (+{} -{})\n",
            shown_path(&file.path),
            file.additions,
            file.deletions
        ));
        if is_secret_path(&file.path) || file.old_path.as_deref().is_some_and(is_secret_path) {
            out.push_str("(protected file, contents not sent)\n");
            continue;
        }
        if file.binary {
            out.push_str("(binary)\n");
        }
        for hunk in &file.hunks {
            out.push_str(&hunk.header);
            out.push('\n');
            for line in hunk.lines.iter().filter(|l| l.kind == LineKind::Added) {
                out.push('+');
                out.push_str(&line.text);
                out.push('\n');
            }
        }
        if out.chars().count() > max_chars {
            let cut: String = out.chars().take(max_chars).collect();
            return format!("{cut}\n… (diff truncated)\n");
        }
    }
    out
}

/// What Hedy asks the task's own agent for "Sugerir con IA".
pub fn commit_message_prompt(title: &str, recent: &[String], diff: &str) -> String {
    let examples = if recent.is_empty() {
        String::new()
    } else {
        let list = recent
            .iter()
            .map(|s| format!("- {s}"))
            .collect::<Vec<_>>()
            .join("\n");
        tr!(
            "\n\nÚltimos commits del repositorio (sigue su idioma y su estilo):\n{list}",
            "\n\nRecent commits of the repository (follow their language and style):\n{list}"
        )
    };
    tr!(
        "Escribe un mensaje de commit de una sola línea, en formato Conventional Commits (`tipo(ámbito): mensaje`, en imperativo, 72 caracteres como mucho), para estos cambios. No uses herramientas ni leas archivos: todo lo que necesitas está aquí. Responde solo con el mensaje, sin comillas ni explicaciones.\n\nPedido: {title}{examples}\n\nCambios:\n{diff}",
        "Write a one-line commit message in Conventional Commits format (`type(scope): message`, imperative, at most 72 characters) for these changes. Do not use tools or read files: everything you need is here. Reply with the message only, without quotes or explanations.\n\nRequest: {title}{examples}\n\nChanges:\n{diff}"
    )
}

/// What `hedy commit` asks for the staged changes: Conventional Commits when
/// most recent subjects use them, otherwise the repository's own style.
/// `secrets` are staged files whose contents were left out (only named).
pub fn staged_commit_prompt(recent: &[String], diff: &str, secrets: &[String]) -> String {
    let format = if uses_conventional_commits(recent.iter().map(String::as_str)) {
        tr!(
            "en formato Conventional Commits (`tipo(ámbito): mensaje`, en imperativo)",
            "in Conventional Commits format (`type(scope): message`, imperative)"
        )
    } else if recent.is_empty() {
        tr!("en imperativo", "in the imperative")
    } else {
        tr!(
            "con el mismo estilo que los últimos commits",
            "in the same style as the recent commits"
        )
    };
    let examples = if recent.is_empty() {
        String::new()
    } else {
        let list = recent
            .iter()
            .map(|s| format!("- {s}"))
            .collect::<Vec<_>>()
            .join("\n");
        tr!(
            "\n\nÚltimos commits del repositorio (sigue su idioma y su estilo):\n{list}",
            "\n\nRecent commits of the repository (follow their language and style):\n{list}"
        )
    };
    let hidden = if secrets.is_empty() {
        String::new()
    } else {
        let list = secrets
            .iter()
            .map(|p| shown_path(p))
            .collect::<Vec<_>>()
            .join(", ");
        tr!(
            "\n\nTambién cambian estos archivos privados, cuyo contenido no se muestra: {list}",
            "\n\nThese private files change too; their contents are not shown: {list}"
        )
    };
    tr!(
        "Escribe un mensaje de commit de una sola línea, {format}, 72 caracteres como mucho, para los cambios preparados (staged) de este repositorio. No uses herramientas ni leas archivos: todo lo que necesitas está aquí. Responde solo con el mensaje, sin comillas ni explicaciones.{examples}\n\nCambios:\n{diff}{hidden}",
        "Write a one-line commit message, {format}, at most 72 characters, for the staged changes of this repository. Do not use tools or read files: everything you need is here. Reply with the message only, without quotes or explanations.{examples}\n\nChanges:\n{diff}{hidden}"
    )
}

/// The one-line message from an agent's answer: the first line that is not
/// a code fence, without quotes, backticks or a "Commit message:" label,
/// at most 100 characters. `None` when nothing is left.
pub fn clean_suggestion(answer: &str) -> Option<String> {
    let line = answer
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("```"))?;
    let mut line = line.trim_matches(|c| matches!(c, '`' | '"' | '\'' | '*' | '«' | '»'));
    for label in [
        "commit message:",
        "mensaje de commit:",
        "mensaje:",
        "message:",
    ] {
        if line.to_lowercase().starts_with(label) {
            line = line[label.len()..].trim();
        }
    }
    let line = line.trim_matches(|c| matches!(c, '`' | '"' | '\'')).trim();
    if line.is_empty() {
        return None;
    }
    Some(line.chars().take(100).collect())
}

/// What a pull request's body says, all of it known locally.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PullRequestFacts<'a> {
    /// What the user asked.
    pub prompt: &'a str,
    /// The agent's final message.
    pub summary: Option<&'a str>,
    pub files: &'a [ShippedFile],
    /// Test commands the task ran, each with its last result.
    pub tests: &'a [(String, bool)],
}

fn shorten(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{cut}…")
}

/// The pull request's body, in the user's language: what was asked, a
/// summary from the agent's final message, the files and the tests run.
pub fn pull_request_body(facts: &PullRequestFacts) -> String {
    let mut out = String::new();
    out.push_str(&tr!("## Qué se pidió\n\n", "## What was asked\n\n"));
    for line in shorten(facts.prompt, 1500).lines() {
        out.push_str("> ");
        out.push_str(line);
        out.push('\n');
    }
    if let Some(summary) = facts.summary.map(str::trim).filter(|s| !s.is_empty()) {
        out.push_str(&tr!("\n## Resumen\n\n", "\n## Summary\n\n"));
        out.push_str(&shorten(summary, 1500));
        out.push('\n');
    }
    let n = facts.files.len();
    out.push_str(&tr!(
        "\n## Archivos cambiados ({n})\n\n",
        "\n## Files changed ({n})\n\n"
    ));
    for file in facts.files.iter().take(50) {
        out.push_str(&format!(
            "- `{}` (+{} −{})\n",
            file.path, file.additions, file.deletions
        ));
    }
    if n > 50 {
        let more = n - 50;
        out.push_str(&tr!("- … y {more} más\n", "- … and {more} more\n"));
    }
    out.push_str("\n## Tests\n\n");
    if facts.tests.is_empty() {
        out.push_str(&tr!(
            "No se ejecutaron tests en esta tarea.\n",
            "No tests were run in this task.\n"
        ));
    }
    for (command, passed) in facts.tests {
        let result = if *passed {
            tr!("pasaron", "passed")
        } else {
            tr!("fallaron", "failed")
        };
        let mark = if *passed { "✅" } else { "❌" };
        out.push_str(&format!("- {mark} `{command}`: {result}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diff::{DiffLine, FileStatus, Hunk};
    use crate::i18n::{Language, with_language};

    #[test]
    fn reads_branch_upstream_and_counts_from_porcelain_v2() {
        let out = "# branch.oid 1234\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\n1 M. N... 100644 100644 100644 a b src/a.rs\n1 .M N... 100644 100644 100644 a b src/b.rs\n1 MM N... 100644 100644 100644 a b src/c.rs\n2 R. N... 100644 100644 100644 a b R100 new.rs\told.rs\nu UU N... 1 2 3 4 a b c conflict.rs\n? notes.txt\n? other.txt\n! ignored\n";
        let folder = parse_status_v2(out);
        assert_eq!(folder.branch.as_deref(), Some("main"));
        assert_eq!(folder.upstream.as_deref(), Some("origin/main"));
        assert_eq!((folder.ahead, folder.behind), (2, 1));
        assert_eq!((folder.staged, folder.unstaged), (3, 2));
        assert_eq!((folder.untracked, folder.conflicted), (2, 1));
        assert_eq!(folder.changed, 7);
    }

    #[test]
    fn a_detached_head_or_a_branch_without_upstream() {
        let folder = parse_status_v2("# branch.oid abc\n# branch.head (detached)\n");
        assert_eq!(folder.branch, None);
        let folder = parse_status_v2("# branch.oid (initial)\n# branch.head nueva\n");
        assert_eq!(folder.branch.as_deref(), Some("nueva"));
        assert_eq!(folder.upstream, None);
        assert_eq!(folder.changed, 0);
    }

    #[test]
    fn the_host_of_a_remote_never_keeps_credentials() {
        for (url, host) in [
            ("https://github.com/hedy/hedy.git", Some("github.com")),
            ("https://user:ghp_secret@github.com/a/b", Some("github.com")),
            ("git@github.com:hedy/hedy.git", Some("github.com")),
            (
                "ssh://git@GitLab.example.com:2222/a/b.git",
                Some("gitlab.example.com"),
            ),
            ("http://[::1]:8080/repo", Some("::1")),
            ("/srv/git/repo.git", None),
            ("../remoto.git", None),
            ("file:///srv/git/repo.git", None),
        ] {
            assert_eq!(remote_host(url).as_deref(), host, "{url}");
        }
    }

    #[test]
    fn picks_origin_or_the_only_remote() {
        let r = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(
            pick_remote(&r(&["upstream", "origin"])).as_deref(),
            Some("origin")
        );
        assert_eq!(pick_remote(&r(&["mio"])).as_deref(), Some("mio"));
        assert_eq!(pick_remote(&r(&["a", "b"])), None);
        assert_eq!(pick_remote(&r(&[])), None);
    }

    #[test]
    fn push_and_pull_failures_are_told_apart() {
        let rejected = " ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs\nhint: Updates were rejected because the remote contains work that you do not have locally.";
        assert_eq!(PushProblem::from_git(rejected), PushProblem::Rejected);
        assert_eq!(
            PushProblem::from_git("fatal: Authentication failed for 'https://github.com/a/b.git/'"),
            PushProblem::Auth
        );
        assert_eq!(
            PushProblem::from_git(
                "git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository."
            ),
            PushProblem::Auth
        );
        assert_eq!(
            PushProblem::from_git(
                "remote: error: GH006: Protected branch update failed for refs/heads/main."
            ),
            PushProblem::Protected
        );
        assert_eq!(
            PushProblem::from_git(
                "fatal: unable to access 'https://github.com/a/b/': Could not resolve host: github.com"
            ),
            PushProblem::Offline
        );
        assert_eq!(PushProblem::from_git("algo raro"), PushProblem::Other);

        assert_eq!(
            PullProblem::from_git(
                "hint: Diverging branches can't be fast-forwarded\nfatal: Not possible to fast-forward, aborting."
            ),
            PullProblem::Diverged
        );
        assert_eq!(
            PullProblem::from_git("There is no tracking information for the current branch."),
            PullProblem::NoUpstream
        );
        assert_eq!(
            PullProblem::from_git(
                "error: Your local changes to the following files would be overwritten by merge:\n\tapp.py"
            ),
            PullProblem::LocalChanges
        );
    }

    #[test]
    fn pull_request_links_come_from_gh_output() {
        let pr = PullRequest::from_gh_output(
            "Creating pull request for hedy/x into main in a/b\n\nhttps://github.com/a/b/pull/42\n",
            true,
        )
        .unwrap();
        assert_eq!(pr.url, "https://github.com/a/b/pull/42");
        assert_eq!((pr.number, pr.draft), (Some(42), true));
        let existing = PullRequest::from_gh_output(
            "a pull request for branch \"hedy/x\" into branch \"main\" already exists:\nhttps://github.com/a/b/pull/7",
            false,
        )
        .unwrap();
        assert_eq!(existing.number, Some(7));
        assert_eq!(
            PullRequest::from_gh_output("http://evil/pull/1", false),
            None
        );
    }

    #[test]
    fn conventional_commits_are_detected_and_suggested() {
        assert!(uses_conventional_commits([
            "feat(app): add x",
            "fix: y",
            "Merge branch 'a'",
            "Update README"
        ]));
        assert!(!uses_conventional_commits(["Add x", "Fix y", "feat: z"]));
        assert!(!uses_conventional_commits(["feat: z"]));
        assert!(!uses_conventional_commits(["Note: something", "Ojo: otra"]));

        assert_eq!(
            conventional_subject("Arregla el cálculo de impuestos…"),
            "fix: arregla el cálculo de impuestos"
        );
        assert_eq!(conventional_subject("Add dark mode"), "feat: add dark mode");
        assert_eq!(
            conventional_subject("Agrega tests para facturas"),
            "test: agrega tests para facturas"
        );
        assert_eq!(conventional_subject("docs: explain"), "docs: explain");
    }

    #[test]
    fn suggestions_are_cleaned_to_one_line() {
        assert_eq!(
            clean_suggestion("```\nfix(facturas): round VAT per line\n```").as_deref(),
            Some("fix(facturas): round VAT per line")
        );
        assert_eq!(
            clean_suggestion("Commit message: `feat: add x`\n\nBecause…").as_deref(),
            Some("feat: add x")
        );
        assert_eq!(clean_suggestion("  \n``` \n"), None);
        assert_eq!(clean_suggestion(&"a".repeat(300)).unwrap().len(), 100);
    }

    fn diff() -> Vec<FileDiff> {
        vec![FileDiff {
            path: "app.py".into(),
            old_path: None,
            status: FileStatus::Modified,
            additions: 1,
            deletions: 1,
            binary: false,
            hunks: vec![Hunk {
                header: "@@ -1,3 +1,3 @@".into(),
                lines: vec![
                    DiffLine {
                        kind: LineKind::Context,
                        old_line: Some(1),
                        new_line: Some(1),
                        text: "def x():".into(),
                    },
                    DiffLine {
                        kind: LineKind::Removed,
                        old_line: Some(2),
                        new_line: None,
                        text: "    pass".into(),
                    },
                    DiffLine {
                        kind: LineKind::Added,
                        old_line: None,
                        new_line: Some(2),
                        text: "    return 1".into(),
                    },
                ],
            }],
        }]
    }

    #[test]
    fn the_diff_for_the_agent_has_only_changed_lines_and_a_cap() {
        let text = diff_for_prompt(&diff(), 10_000);
        assert_eq!(text, "--- app.py (+1 -1)\n@@ -1,3 +1,3 @@\n+    return 1\n");
        assert!(diff_for_prompt(&diff(), 10).ends_with("(diff truncated)\n"));
        // A protected file goes by name; a path's control characters are shown.
        let mut secret = diff();
        secret[0].path = "conf/.env.\u{1b}]0;x".into();
        let text = diff_for_prompt(&secret, 10_000);
        assert!(!text.contains("return 1") && !text.contains('\u{1b}'));
        assert!(text.contains("(protected file, contents not sent)"));
        let prompt = commit_message_prompt("Arregla x", &["feat: a".into()], &text);
        assert!(prompt.contains("Conventional Commits"));
        assert!(prompt.contains("- feat: a"));
        assert!(prompt.ends_with(&text));
    }

    #[test]
    fn the_pull_request_body_tells_what_was_asked_done_and_tested() {
        let files = [ShippedFile {
            path: "app.py".into(),
            additions: 3,
            deletions: 1,
        }];
        let tests = [("pytest -q".to_owned(), true)];
        let body = with_language(Language::English, || {
            pull_request_body(&PullRequestFacts {
                prompt: "Fix the VAT\nper line",
                summary: Some("VAT is rounded per line."),
                files: &files,
                tests: &tests,
            })
        });
        assert_eq!(
            body,
            "## What was asked\n\n> Fix the VAT\n> per line\n\n## Summary\n\nVAT is rounded per line.\n\n## Files changed (1)\n\n- `app.py` (+3 −1)\n\n## Tests\n\n- ✅ `pytest -q`: passed\n"
        );
        let body = pull_request_body(&PullRequestFacts {
            prompt: "Arregla",
            ..Default::default()
        });
        assert!(body.contains("No se ejecutaron tests"));
        assert!(!body.contains("Resumen"));
        assert_eq!(commit_subject("\n  fix: x  \n\ncuerpo"), "fix: x");
    }
}
