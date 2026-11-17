//! Claude Code `PreToolUse` hook. Claude Code runs some tools without asking
//! (file reads, read-only shell commands such as `cat`), so Hedy's permission
//! cards never see them. The hook does: Claude Code calls it before every tool
//! and it denies anything touching a protected file.
//!
//! A host binary (the app, the smoke example) runs it when started with
//! [`HOOK_ARGS`]: `program hook pre-tool-use`, JSON on stdin, JSON on stdout.
//!
//! With the `read-guard` experiment it also denies reads that add nothing
//! ([`crate::domain::read_guard`]). Each call is a new process, so the reads
//! of a session are kept in a small file per session in the temp folder.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::domain::permissions::protected_file_reason;
use crate::domain::read_guard::{self, FileStamp, ReadKey, ReadRecord};
use crate::experiments::{self, Experiment};

pub const HOOK_ARGS: [&str; 2] = ["hook", "pre-tool-use"];

/// Tools the hook is registered for.
pub const HOOK_MATCHER: &str = "Bash|Read|Edit|Write|MultiEdit|NotebookEdit|Grep|Glob";

/// Hook output for a tool call, or `None` to let Claude Code decide as usual.
pub fn decide(input: &str) -> Option<String> {
    let guard = experiments::on(Experiment::ReadGuard).then(guard_dir);
    decide_with(input, guard.as_deref())
}

/// Where the read guard keeps each session's reads.
fn guard_dir() -> PathBuf {
    std::env::temp_dir().join("hedy-read-guard")
}

/// [`decide`]; with `guard`, the read guard keeps its sessions there.
fn decide_with(input: &str, guard: Option<&Path>) -> Option<String> {
    let input: Value = serde_json::from_str(input).ok()?;
    let tool = input["tool_name"].as_str()?;
    let cwd = PathBuf::from(input["cwd"].as_str().unwrap_or("/"));
    let reason = protected_file_reason(tool, &input["tool_input"], &cwd).or_else(|| {
        let dir = guard?;
        (tool == "Read").then(|| guard_read(&input, &cwd, dir))?
    })?;
    Some(
        json!({"hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }})
        .to_string(),
    )
}

/// The read guard's answer for a `Read`, recording it in the session's file.
/// Anything it cannot check (no session, a missing file) goes through.
fn guard_read(input: &Value, cwd: &Path, dir: &Path) -> Option<String> {
    let session = input["session_id"].as_str()?;
    // A file name, not a path: the id comes from the CLI.
    if session.is_empty()
        || !session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return None;
    }
    let tool_input = &input["tool_input"];
    let path = cwd.join(tool_input["file_path"].as_str()?);
    let meta = std::fs::metadata(&path).ok()?;
    let stamp = FileStamp {
        size: meta.len(),
        modified_ns: meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos(),
    };
    let key = ReadKey {
        path: path.to_string_lossy().into_owned(),
        offset: tool_input["offset"].as_u64(),
        limit: tool_input["limit"].as_u64(),
    };
    let log = dir.join(format!("{session}.jsonl"));
    let history: Vec<ReadRecord> = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let reason = read_guard::deny_reason(&key, stamp, &history);
    let record = ReadRecord {
        key,
        stamp,
        denied: reason.is_some(),
    };
    if std::fs::create_dir_all(dir).is_ok()
        && let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
        && let Ok(line) = serde_json::to_string(&record)
    {
        let _ = writeln!(file, "{line}");
    }
    reason
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
    fn the_read_guard_denies_an_unchanged_reread_once() {
        let tmp = tempfile::tempdir().unwrap();
        let (wt, guard) = (tmp.path().join("wt"), tmp.path().join("guard"));
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join("app.py"), "print(1)\n").unwrap();
        let read = |session: &str| {
            let payload = json!({"hook_event_name": "PreToolUse", "cwd": wt, "session_id": session,
                "tool_name": "Read", "tool_input": {"file_path": "app.py"}});
            decide_with(&payload.to_string(), Some(&guard))
        };
        assert!(read("s1").is_none());
        let denied: Value = serde_json::from_str(&read("s1").unwrap()).unwrap();
        let reason = denied["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap();
        assert!(reason.contains("releer «app.py»"), "{reason}");
        // It insists; another session reads it fresh.
        assert!(read("s1").is_none());
        assert!(read("s2").is_none());
        // A bad session id is not a file name: no guard.
        assert!(read("../x").is_none() && read("../x").is_none());
        // Off: nothing is recorded or denied.
        let payload = json!({"cwd": wt, "session_id": "s1", "tool_name": "Read",
            "tool_input": {"file_path": "app.py"}});
        assert!(decide_with(&payload.to_string(), None).is_none());
    }

    #[test]
    fn command_is_shell_quoted() {
        assert_eq!(
            command(Path::new("/opt/My App/hedy's")),
            r"'/opt/My App/hedy'\''s' hook pre-tool-use"
        );
    }
}
