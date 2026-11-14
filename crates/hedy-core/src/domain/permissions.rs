//! Which agent tool requests are answered automatically and which become a
//! permission card in the UI.

use std::path::{Component, Path, PathBuf};

use serde_json::Value;

/// File names agents must never touch, in any project.
pub const DEFAULT_BLOCKED: &[&str] = &[
    ".env",
    ".env.*",
    "*.pem",
    "*.key",
    "id_rsa*",
    "id_ed25519*",
    ".npmrc",
    ".pypirc",
    "credentials*",
];

/// Remembered by "Permitir en este chat".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowRule {
    /// Any use of this tool (e.g. every `Edit`).
    Tool(String),
    /// Shell commands starting with this program (e.g. `pytest`).
    Command(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Show a card and wait for the user.
    Ask,
    Allow,
    Deny(String),
}

const FILE_TOOLS: &[&str] = &["Read", "Edit", "Write", "MultiEdit", "NotebookEdit"];
const WRITE_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// `*` matches any run of characters; the match is on the whole file name.
fn wildcard(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == name,
        Some((head, tail)) => {
            name.starts_with(head)
                && name.len() >= head.len()
                && (0..=name.len() - head.len())
                    .filter(|i| name.is_char_boundary(head.len() + i))
                    .any(|i| wildcard(tail, &name[head.len() + i..]))
        }
    }
}

pub fn is_blocked_name(name: &str, blocked: &[String]) -> bool {
    blocked.iter().any(|p| wildcard(p, name))
}

/// Lexically normalized `path` (relative paths resolved against `base`).
fn normalize(base: &Path, path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    };
    let mut out = PathBuf::new();
    for part in joined.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

fn file_path(input: &Value) -> Option<&str> {
    input["file_path"]
        .as_str()
        .or_else(|| input["notebook_path"].as_str())
}

/// Every file a request touches: `file_path`, plus `paths` for multi-file
/// changes (a Codex patch).
fn file_paths(input: &Value) -> Vec<&str> {
    file_path(input)
        .into_iter()
        .chain(
            input["paths"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        )
        .collect()
}

/// The program of a shell command: `FOO=1 pytest -q` → `pytest`.
pub fn command_program(command: &str) -> Option<&str> {
    command.split_whitespace().find(|word| !word.contains('='))
}

/// What "Permitir en este chat" remembers for this request.
pub fn rule_for(tool: &str, input: &Value) -> AllowRule {
    if tool == "Bash"
        && let Some(program) = input["command"].as_str().and_then(command_program)
    {
        return AllowRule::Command(program.to_owned());
    }
    AllowRule::Tool(tool.to_owned())
}

/// Decides a tool request. `work_dir` is the task's worktree: writes outside it
/// are denied, and blocked files are never read or written.
pub fn evaluate(
    tool: &str,
    input: &Value,
    work_dir: &Path,
    blocked: &[String],
    remembered: &[AllowRule],
) -> Verdict {
    if let Some(reason) = protected_file_reason(tool, input, work_dir, blocked) {
        return Verdict::Deny(reason);
    }
    if WRITE_TOOLS.contains(&tool)
        && file_paths(input)
            .iter()
            .any(|path| !normalize(work_dir, Path::new(path)).starts_with(work_dir))
    {
        return Verdict::Deny(format!(
            "Hedy solo permite cambios dentro de la carpeta de la tarea ({}).",
            work_dir.display()
        ));
    }
    if remembered.contains(&rule_for(tool, input)) {
        return Verdict::Allow;
    }
    Verdict::Ask
}

/// Why `tool` may not run because it touches a protected file, if it does.
/// Also used by the `PreToolUse` hook, which sees the tools Claude Code runs
/// without asking (reads, read-only shell commands).
pub fn protected_file_reason(
    tool: &str,
    input: &Value,
    work_dir: &Path,
    blocked: &[String],
) -> Option<String> {
    if FILE_TOOLS.contains(&tool) {
        for path in file_paths(input) {
            let path = normalize(work_dir, Path::new(path));
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if is_blocked_name(name, blocked) {
                return Some(format!(
                    "Hedy no permite tocar «{name}» (archivo protegido)."
                ));
            }
        }
    }
    if tool == "Bash"
        && let Some(command) = input["command"].as_str()
        && let Some(name) = command
            .split(|c: char| {
                c.is_whitespace()
                    || matches!(
                        c,
                        '<' | '>' | '|' | ';' | '&' | '\'' | '"' | '=' | '(' | ')'
                    )
            })
            .filter_map(|word| Path::new(word).file_name()?.to_str())
            .find(|name| is_blocked_name(name, blocked))
    {
        return Some(format!(
            "Hedy no permite comandos sobre «{name}» (archivo protegido)."
        ));
    }
    None
}

/// One-line description of a request for the permission card.
pub fn summarize(tool: &str, input: &Value) -> String {
    let field = |key: &str| input[key].as_str().map(str::to_owned);
    match tool {
        "Bash" => field("command"),
        "Edit" | "Write" | "MultiEdit" | "Read" | "NotebookEdit" => {
            file_path(input).map(str::to_owned)
        }
        "WebFetch" => field("url"),
        "WebSearch" => field("query"),
        "Grep" | "Glob" => field("pattern"),
        _ => None,
    }
    .unwrap_or_else(|| tool.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn blocked() -> Vec<String> {
        DEFAULT_BLOCKED.iter().map(|s| s.to_string()).collect()
    }

    const WT: &str = "/repo/.hedy/worktrees/t1";

    fn verdict(tool: &str, input: Value, remembered: &[AllowRule]) -> Verdict {
        evaluate(tool, &input, Path::new(WT), &blocked(), remembered)
    }

    #[test]
    fn wildcard_matching() {
        assert!(wildcard(".env.*", ".env.local"));
        assert!(wildcard("*.pem", "cert.pem"));
        assert!(wildcard("id_rsa*", "id_rsa"));
        assert!(!wildcard(".env.*", ".env"));
        assert!(!wildcard("*.pem", "cert.pem.txt"));
        assert!(wildcard("*", "ñandú"));
    }

    #[test]
    fn blocked_files_are_denied_for_reads_writes_and_commands() {
        assert!(matches!(
            verdict("Read", json!({"file_path": format!("{WT}/.env")}), &[]),
            Verdict::Deny(_)
        ));
        assert!(matches!(
            verdict("Edit", json!({"file_path": "config/.env.production"}), &[]),
            Verdict::Deny(_)
        ));
        assert!(matches!(
            verdict("Bash", json!({"command": "cat .env | grep KEY"}), &[]),
            Verdict::Deny(_)
        ));
        assert!(matches!(
            verdict("Bash", json!({"command": "cp ~/.ssh/id_rsa /tmp"}), &[]),
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn writes_outside_the_worktree_are_denied() {
        let outside = verdict("Write", json!({"file_path": "/repo/src/main.rs"}), &[]);
        assert!(matches!(outside, Verdict::Deny(m) if m.contains("carpeta de la tarea")));
        let escape = verdict("Edit", json!({"file_path": "../../../src/x.rs"}), &[]);
        assert!(matches!(escape, Verdict::Deny(_)));
        // Reads outside are fine (e.g. a library's source).
        assert_eq!(
            verdict("Read", json!({"file_path": "/usr/include/stdio.h"}), &[]),
            Verdict::Ask
        );
        assert_eq!(
            verdict("Edit", json!({"file_path": format!("{WT}/src/a.rs")}), &[]),
            Verdict::Ask
        );
    }

    #[test]
    fn multi_file_changes_are_checked_file_by_file() {
        let patch = json!({"file_path": format!("{WT}/a.py"), "paths": [format!("{WT}/a.py"), format!("{WT}/.env")]});
        assert!(matches!(verdict("Edit", patch, &[]), Verdict::Deny(_)));
        let escape = json!({"file_path": format!("{WT}/a.py"), "paths": ["/etc/passwd"]});
        assert!(matches!(verdict("Edit", escape, &[]), Verdict::Deny(_)));
    }

    #[test]
    fn remembered_rules_allow_matching_requests() {
        let rules = vec![
            AllowRule::Tool("Edit".into()),
            AllowRule::Command("pytest".into()),
        ];
        assert_eq!(
            verdict("Edit", json!({"file_path": "src/a.py"}), &rules),
            Verdict::Allow
        );
        assert_eq!(
            verdict("Bash", json!({"command": "pytest -q tests"}), &rules),
            Verdict::Allow
        );
        assert_eq!(
            verdict("Bash", json!({"command": "rm -rf build"}), &rules),
            Verdict::Ask
        );
        // Rules never override protected files.
        assert!(matches!(
            verdict("Edit", json!({"file_path": ".env"}), &rules),
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn rules_and_summaries() {
        assert_eq!(
            rule_for("Bash", &json!({"command": "CI=1 npm test"})),
            AllowRule::Command("npm".into())
        );
        assert_eq!(
            rule_for("Write", &json!({})),
            AllowRule::Tool("Write".into())
        );
        assert_eq!(summarize("Bash", &json!({"command": "ls"})), "ls");
        assert_eq!(summarize("Edit", &json!({"file_path": "a.rs"})), "a.rs");
        assert_eq!(summarize("Mystery", &json!({})), "Mystery");
    }
}
