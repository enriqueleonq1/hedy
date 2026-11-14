//! Claude Code `PreToolUse` hook. Claude Code runs some tools without asking
//! (file reads, read-only shell commands such as `cat`), so Hedy's permission
//! cards never see them. The hook does: Claude Code calls it before every tool
//! and it denies anything touching a protected file.
//!
//! A host binary (the app, the smoke example) runs it when started with
//! [`HOOK_ARGS`]: `program hook pre-tool-use`, JSON on stdin, JSON on stdout.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::domain::permissions::protected_file_reason;

pub const HOOK_ARGS: [&str; 2] = ["hook", "pre-tool-use"];

/// Tools the hook is registered for.
pub const HOOK_MATCHER: &str = "Bash|Read|Edit|Write|MultiEdit|NotebookEdit|Grep|Glob";

/// Hook output for a tool call, or `None` to let Claude Code decide as usual.
pub fn decide(input: &str) -> Option<String> {
    let input: Value = serde_json::from_str(input).ok()?;
    let tool = input["tool_name"].as_str()?;
    let cwd = PathBuf::from(input["cwd"].as_str().unwrap_or("/"));
    let reason = protected_file_reason(tool, &input["tool_input"], &cwd)?;
    Some(
        json!({"hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }})
        .to_string(),
    )
}

/// Runs the hook on this process's stdin/stdout. Returns the exit code.
pub fn run() -> i32 {
    crate::i18n::init(None);
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return 0;
    }
    let output = decide(&input);
    // Optional trace for debugging and for the smoke test.
    if let Some(log) = std::env::var_os("HEDY_HOOK_LOG")
        && let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
    {
        let _ = writeln!(
            file,
            "{} -> {}",
            input.trim(),
            output.as_deref().unwrap_or("ask")
        );
    }
    if let Some(output) = output {
        println!("{output}");
    }
    0
}

/// Shell command Claude Code runs for the hook: `'/path/to/program' hook pre-tool-use`.
pub fn command(program: &Path) -> String {
    let quoted = format!("'{}'", program.to_string_lossy().replace('\'', r"'\''"));
    std::iter::once(quoted)
        .chain(HOOK_ARGS.iter().map(|a| a.to_string()))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(tool: &str, input: Value) -> Option<Value> {
        let payload = json!({"hook_event_name": "PreToolUse", "cwd": "/wt", "tool_name": tool, "tool_input": input});
        decide(&payload.to_string()).map(|s| serde_json::from_str(&s).unwrap())
    }

    #[test]
    fn denies_protected_files_even_for_auto_allowed_tools() {
        let out = hook("Bash", json!({"command": "cat .env"})).unwrap();
        assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "deny");
        assert!(
            out["hookSpecificOutput"]["permissionDecisionReason"]
                .as_str()
                .unwrap()
                .contains(".env")
        );
        assert!(
            hook(
                "Bash",
                json!({"command": "source ./.env.local && npm test"})
            )
            .is_some()
        );
        assert!(hook("Read", json!({"file_path": "/wt/config/.env"})).is_some());
    }

    #[test]
    fn leaves_everything_else_to_claude_code() {
        assert!(hook("Bash", json!({"command": "cat README.md"})).is_none());
        // Writes outside the worktree go through the permission cards, and
        // plan mode must keep writing its plan file.
        assert!(hook("Write", json!({"file_path": "/home/e/.claude/plans/p.md"})).is_none());
        assert!(decide("not json").is_none());
    }

    #[test]
    fn command_is_shell_quoted() {
        assert_eq!(
            command(Path::new("/opt/My App/hedy's")),
            r"'/opt/My App/hedy'\''s' hook pre-tool-use"
        );
    }
}
