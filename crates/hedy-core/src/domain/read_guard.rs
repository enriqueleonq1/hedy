//! The `read-guard` experiment (docs/decisions.md §33): reads that spend
//! tokens without adding anything. Claude's `PreToolUse` hook asks this before
//! each `Read`:
//! - a file read again in the same session, with the same range, that has not
//!   changed since: the agent already has it;
//! - lockfiles and minified files: generated, long and rarely useful whole;
//! - a big file read whole: the agent is told to read a range or search it.
//!
//! The agent can insist: a read denied once goes through the next time, so a
//! guard that guessed wrong costs one turn, not the task.

use serde::{Deserialize, Serialize};

use crate::tr;

/// Bigger than this, a whole read is denied (Claude Code itself stops at
/// about 2,000 lines; this is far below that, around 1,500 lines of code).
pub const BIG_FILE_BYTES: u64 = 64 * 1024;

/// Generated files nobody reads whole.
const GENERATED: &[&str] = &[
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lockb",
    "Cargo.lock",
    "poetry.lock",
    "Pipfile.lock",
    "uv.lock",
    "composer.lock",
    "Gemfile.lock",
    "go.sum",
];

/// One `Read` call: which file and which part of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadKey {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

impl ReadKey {
    fn whole(&self) -> bool {
        self.offset.is_none() && self.limit.is_none()
    }
}

/// What the file looked like at the time of a read: when it changes, it may
/// be read again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStamp {
    pub size: u64,
    pub modified_ns: u128,
}

/// A read the guard has seen in this session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadRecord {
    pub key: ReadKey,
    pub stamp: FileStamp,
    /// The guard denied it (the next identical read goes through).
    pub denied: bool,
}

/// Why a read is denied; `None` lets it through. `history` is this
/// session's reads, oldest first.
pub fn deny_reason(key: &ReadKey, stamp: FileStamp, history: &[ReadRecord]) -> Option<String> {
    let name = file_name(&key.path);
    let last = history.iter().rev().find(|r| r.key == *key);
    // The agent insisted after a denial: let it read.
    if last.is_some_and(|r| r.denied && r.stamp == stamp) {
        return None;
    }
    if last.is_some_and(|r| r.stamp == stamp) {
        return Some(tr!(
            "Hedy no permite releer «{name}»: ya lo leíste en esta sesión y no cambió desde entonces. Usa lo que ya leíste; si de verdad lo necesitas otra vez, vuelve a pedirlo.",
            "Hedy does not allow re-reading «{name}»: you already read it in this session and it has not changed since. Use what you read; if you really need it again, ask again."
        ));
    }
    if !key.whole() {
        return None;
    }
    if is_generated(name) {
        return Some(tr!(
            "Hedy no permite leer «{name}» entero: es un archivo generado. Busca lo que necesitas con Grep, o lee un rango con offset y limit.",
            "Hedy does not allow reading «{name}» whole: it is a generated file. Search it with Grep, or read a range with offset and limit."
        ));
    }
    if stamp.size > BIG_FILE_BYTES {
        let kb = stamp.size / 1024;
        return Some(tr!(
            "Hedy no permite leer «{name}» entero ({kb} KB). Busca lo que necesitas con Grep, o lee un rango con offset y limit.",
            "Hedy does not allow reading «{name}» whole ({kb} KB). Search it with Grep, or read a range with offset and limit."
        ));
    }
    None
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Lockfiles and minified code.
pub fn is_generated(name: &str) -> bool {
    GENERATED.contains(&name)
        || [".min.js", ".min.css", ".min.mjs", ".map"]
            .iter()
            .any(|ext| name.ends_with(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(path: &str) -> ReadKey {
        ReadKey {
            path: path.into(),
            offset: None,
            limit: None,
        }
    }

    const SMALL: FileStamp = FileStamp {
        size: 900,
        modified_ns: 1,
    };

    fn read(key: ReadKey, stamp: FileStamp, denied: bool) -> ReadRecord {
        ReadRecord { key, stamp, denied }
    }

    #[test]
    fn a_first_read_goes_through() {
        assert_eq!(deny_reason(&key("/wt/src/app.py"), SMALL, &[]), None);
    }

    #[test]
    fn denies_rereading_an_unchanged_file_once() {
        let history = vec![read(key("/wt/src/app.py"), SMALL, false)];
        let reason = deny_reason(&key("/wt/src/app.py"), SMALL, &history).unwrap();
        assert!(reason.contains("Hedy no permite releer «app.py»"));

        // Changed since (an edit): it may be read again.
        let edited = FileStamp {
            size: 950,
            modified_ns: 2,
        };
        assert_eq!(deny_reason(&key("/wt/src/app.py"), edited, &history), None);
        // Another range of the same file is another read.
        let range = ReadKey {
            offset: Some(100),
            limit: Some(50),
            ..key("/wt/src/app.py")
        };
        assert_eq!(deny_reason(&range, SMALL, &history), None);

        // The agent insisted after the denial: let it through.
        let mut insisted = history.clone();
        insisted.push(read(key("/wt/src/app.py"), SMALL, true));
        assert_eq!(deny_reason(&key("/wt/src/app.py"), SMALL, &insisted), None);
    }

    #[test]
    fn denies_whole_generated_and_big_files() {
        assert!(
            deny_reason(&key("/wt/package-lock.json"), SMALL, &[])
                .unwrap()
                .contains("archivo generado")
        );
        assert!(deny_reason(&key("/wt/web/app.min.js"), SMALL, &[]).is_some());
        let big = FileStamp {
            size: 200 * 1024,
            modified_ns: 1,
        };
        assert!(
            deny_reason(&key("/wt/data/report.csv"), big, &[])
                .unwrap()
                .contains("(200 KB)")
        );
        // A range of them is fine.
        let range = ReadKey {
            limit: Some(200),
            ..key("/wt/Cargo.lock")
        };
        assert_eq!(deny_reason(&range, big, &[]), None);
        assert!(!is_generated("lock.py") && is_generated("Cargo.lock"));
    }
}
