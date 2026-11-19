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
use std::rc::Rc;

use serde_json::{Value, json};

use crate::domain::permissions::{
    is_blocked_name, matches, protected_file_reason, whole_search_root,
};
use crate::domain::read_guard::{self, FileStamp, ReadKey, ReadRecord};
use crate::experiments::{self, Experiment};
use crate::tr;

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
    let reason = protected_file_reason(tool, &input["tool_input"], &cwd)
        .or_else(|| linked_protected(&input["tool_input"], &cwd))
        .or_else(|| {
            let root = whole_search_root(tool, &input["tool_input"], &cwd)?;
            let respect = input["tool_input"]["command"]
                .as_str()
                .is_some_and(respects_gitignore);
            Some(match protected_inside(&root, respect)? {
                Inside::Protected(name) => tr!(
                    "La búsqueda leería «{name}» (archivo protegido) sin nombrarlo: acótala con un glob (por ejemplo *.ts), o pide solo los nombres de los archivos.",
                    "The search would read «{name}» (protected file) without naming it: narrow it with a glob (for example *.ts), or ask only for file names."
                ),
                Inside::TooLarge => tr!(
                    "La búsqueda recorre demasiadas carpetas para comprobar que no lee un archivo protegido: acótala con un glob (por ejemplo *.ts) o una carpeta.",
                    "The search covers too many folders to check it reads no protected file: narrow it with a glob (for example *.ts) or a folder."
                ),
            })
        })
        .or_else(|| {
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

/// Folders too large to look through that hold no project secrets: git's
/// own, and installed dependencies.
const SKIPPED_DIRS: &[&str] = &[".git", "node_modules"];
/// How deep and how many entries the look for a protected file goes; past
/// either it says there may be one (a search that large is narrowed first).
const PROTECTED_DEPTH: usize = 12;
const PROTECTED_ENTRIES: usize = 20_000;

/// A file the tool names that is a link to a protected file (`docs/setup.txt`
/// → `../.env`): the name alone says nothing.
fn linked_protected(input: &Value, cwd: &Path) -> Option<String> {
    ["file_path", "notebook_path", "path"]
        .iter()
        .filter_map(|key| input[*key].as_str())
        .find_map(|path| {
            let real = std::fs::canonicalize(cwd.join(path)).ok()?;
            let name = real.file_name()?.to_string_lossy().into_owned();
            is_blocked_name(&name).then(|| {
                tr!(
                    "Hedy no permite tocar «{name}» (archivo protegido), tampoco por un enlace.",
                    "Hedy does not allow touching «{name}» (protected file), not through a link either."
                )
            })
        })
}

/// Whether a shell search leaves out what git ignores, as the walk may:
/// only ripgrep and ag do, and not with `--no-ignore…`, `--unrestricted` or
/// a `u` among its short flags (`-uu`, `-uL`). grep, ack, `git grep` and
/// the Grep tool are taken to read everything.
fn respects_gitignore(command: &str) -> bool {
    let words: Vec<&str> = command.split_whitespace().collect();
    let program = |w: &&str| w.rsplit('/').next().unwrap_or(w).to_owned();
    let readers_of_all = ["grep", "egrep", "fgrep", "rgrep", "ugrep", "ack", "git"];
    words
        .iter()
        .any(|w| matches!(program(w).as_str(), "rg" | "ag"))
        && !words
            .iter()
            .any(|w| readers_of_all.contains(&program(w).as_str()))
        && !words.iter().any(|w| {
            w.starts_with("--no-ignore")
                || w.starts_with("--unrestricted")
                || w.starts_with("--skip-vcs-ignores")
                || (w.starts_with('-') && !w.starts_with("--") && w.contains('u'))
        })
}

/// What a whole search would read under its folder.
#[derive(Debug, PartialEq, Eq)]
enum Inside {
    /// A protected file, by name.
    Protected(String),
    /// More than can be checked: the search is narrowed first.
    TooLarge,
}

/// The `.gitignore` names (`target`, `dist/`, `*.log`) of a folder, added to
/// its parent's: a search skips what git ignores, and so does the walk.
fn ignored_names(dir: &Path, parent: &Rc<Vec<String>>) -> Rc<Vec<String>> {
    let Ok(text) = std::fs::read_to_string(dir.join(".gitignore")) else {
        return parent.clone();
    };
    let mut names = (**parent).clone();
    names.extend(text.lines().filter_map(|line| {
        let line = line.trim().trim_start_matches('/').trim_end_matches('/');
        // A path inside (`a/b`) or a negation is left to git: not skipped.
        // Only wildcards (`*`) would hide folders that `!` lines bring back.
        let wildcard_only = line.chars().all(|c| matches!(c, '*' | '?'));
        (!line.is_empty() && !line.starts_with(['#', '!']) && !line.contains('/') && !wildcard_only)
            .then(|| line.to_owned())
    }));
    Rc::new(names)
}

/// A protected file under `root` (a file `root` itself counts too), looking
/// where a search would: not in what git ignores. Past its limits it cannot
/// tell, and says so.
fn protected_inside(root: &Path, respect_ignored: bool) -> Option<Inside> {
    if root.is_file() {
        let name = root.file_name()?.to_string_lossy().into_owned();
        return is_blocked_name(&name).then_some(Inside::Protected(name));
    }
    let base = Rc::new(
        std::fs::read_to_string(root.join(".git/info/exclude"))
            .map(|text| {
                text.lines()
                    .map(|l| {
                        l.trim()
                            .trim_start_matches('/')
                            .trim_end_matches('/')
                            .to_owned()
                    })
                    .filter(|l| !l.is_empty() && !l.starts_with(['#', '!']) && !l.contains('/'))
                    .collect()
            })
            .unwrap_or_default(),
    );
    let mut stack = vec![(root.to_path_buf(), 0, ignored_names(root, &base))];
    let mut seen = 0;
    // Folders reached through a link too (`rg -L`), each once.
    let mut visited = std::collections::HashSet::new();
    while let Some((dir, depth, ignored)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            // A protected file is never skipped, even if git ignores it.
            if is_blocked_name(&name) {
                return Some(Inside::Protected(name));
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let skipped = SKIPPED_DIRS.contains(&name.as_str())
                || (respect_ignored && ignored.iter().any(|pattern| matches(pattern, &name)));
            let folder = if kind.is_dir() {
                Some(entry.path())
            } else if kind.is_symlink()
                && let Ok(real) = std::fs::canonicalize(entry.path())
            {
                if !real.is_dir() {
                    let target = real.file_name()?.to_string_lossy().into_owned();
                    if is_blocked_name(&target) {
                        return Some(Inside::Protected(target));
                    }
                    None
                } else {
                    visited.insert(real.clone()).then_some(real)
                }
            } else {
                None
            };
            let Some(folder) = folder.filter(|_| !skipped) else {
                continue;
            };
            seen += 1;
            if seen > PROTECTED_ENTRIES || depth >= PROTECTED_DEPTH {
                return Some(Inside::TooLarge);
            }
            let names = ignored_names(&folder, &ignored);
            stack.push((folder, depth + 1, names));
        }
        seen += 1;
        if seen > PROTECTED_ENTRIES {
            return Some(Inside::TooLarge);
        }
    }
    None
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
    fn a_whole_search_over_a_protected_file_is_denied() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::create_dir_all(dir.join("src/config")).unwrap();
        std::fs::write(dir.join("src/app.ts"), "const key = process.env.KEY;").unwrap();
        let input = |tool: &str, tool_input: Value| {
            json!({"tool_name": tool, "tool_input": tool_input, "cwd": dir}).to_string()
        };
        let content = json!({"pattern": "KEY", "output_mode": "content"});
        // Nothing protected inside: Claude Code decides.
        assert_eq!(decide_with(&input("Grep", content.clone()), None), None);
        std::fs::write(dir.join("src/config/.env"), "KEY=secreto").unwrap();
        let denied = decide_with(&input("Grep", content), None).unwrap();
        assert!(
            denied.contains("\"deny\"") && denied.contains(".env"),
            "{denied}"
        );
        assert!(decide_with(&input("Bash", json!({"command": "rg KEY"})), None).is_some());
        // Narrowed, or names only: fine.
        assert_eq!(
            decide_with(
                &input(
                    "Grep",
                    json!({"pattern": "KEY", "output_mode": "content", "glob": "*.ts"})
                ),
                None
            ),
            None
        );
        assert_eq!(
            decide_with(&input("Grep", json!({"pattern": "KEY"})), None),
            None
        );
        // A link to it reads it too.
        std::os::unix::fs::symlink(dir.join("src/config/.env"), dir.join("docs.txt")).unwrap();
        let read = |file: &str| input("Read", json!({"file_path": file}));
        assert!(decide_with(&read("docs.txt"), None).is_some());
        assert_eq!(decide_with(&read("src/app.ts"), None), None);
        std::fs::remove_file(dir.join("docs.txt")).unwrap();
        // In a folder reached through a link (`rg -L`), and with a loop.
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join(".env"), "X=1").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.join("lnk")).unwrap();
        std::os::unix::fs::symlink(dir, dir.join("src/loop")).unwrap();
        std::fs::rename(dir.join("src/config/.env"), dir.join("src/config/env.bak")).unwrap();
        assert!(decide_with(&input("Bash", json!({"command": "rg -L K"})), None).is_some());
        std::fs::remove_file(dir.join("lnk")).unwrap();
        std::fs::rename(dir.join("src/config/env.bak"), dir.join("src/config/.env")).unwrap();
        // A large build folder git ignores is not looked through.
        std::fs::write(dir.join(".gitignore"), "target/\n").unwrap();
        for i in 0..(PROTECTED_ENTRIES + 10) {
            std::fs::create_dir_all(dir.join(format!("target/d{}/e{i}", i % 50))).unwrap();
        }
        std::fs::remove_file(dir.join("src/config/.env")).unwrap();
        let rg = || decide_with(&input("Bash", json!({"command": "rg KEY"})), None);
        assert_eq!(rg(), None);
        // grep, ack and `git grep` read what git ignores: never skipped for them.
        std::fs::write(dir.join("target/d1/.env"), "K=1").unwrap();
        for command in [
            "grep -r KEY .",
            "rg -uL KEY",
            "git grep --no-index KEY",
            "rg --unrestricted KEY",
            "ack KEY",
        ] {
            assert!(
                decide_with(&input("Bash", json!({"command": command})), None).is_some(),
                "{command}"
            );
        }
        std::fs::remove_file(dir.join("target/d1/.env")).unwrap();
        // Unless the search reads what git ignores.
        std::fs::write(dir.join("target/d0/.env"), "K=1").unwrap();
        assert!(
            decide_with(
                &input("Bash", json!({"command": "rg --no-ignore KEY"})),
                None
            )
            .is_some()
        );
        assert!(decide_with(&input("Bash", json!({"command": "rg -uu KEY"})), None).is_some());
        std::fs::remove_file(dir.join("target/d0/.env")).unwrap();
        // Not ignored, it is too large to check: narrowed first, no file named.
        std::fs::remove_file(dir.join(".gitignore")).unwrap();
        let denied = rg().unwrap();
        assert!(
            denied.contains("demasiadas") || denied.contains("too many"),
            "{denied}"
        );
        std::fs::remove_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("src/config/.env"), "KEY=secreto").unwrap();
        // A protected file in a skipped folder (dependencies) does not count.
        std::fs::remove_file(dir.join("src/config/.env")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules/x")).unwrap();
        std::fs::write(dir.join("node_modules/x/.env"), "X=1").unwrap();
        assert_eq!(
            decide_with(&input("Bash", json!({"command": "rg KEY"})), None),
            None
        );
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
