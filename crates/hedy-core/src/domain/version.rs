//! Reading a CLI's version from what `--version` prints.

/// The first version number in `output`: `2.1.306 (Claude Code)` → `2.1.306`,
/// `codex-cli 0.169.0` → `0.169.0`, `kimi, version 1.2.0` → `1.2.0`. A
/// leading `v` is dropped. `None` when no `major.minor` number is found.
pub fn parse_version(output: &str) -> Option<String> {
    output
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '(' | ')' | '/' | '@'))
        .map(|word| word.trim_end_matches(['.', ':', ';']))
        .map(|word| word.strip_prefix(['v', 'V']).unwrap_or(word))
        .find(|word| looks_like_version(word))
        .map(str::to_owned)
}

/// The first Claude Code verified to take
/// `--exclude-dynamic-system-prompt-sections`, the part of the
/// context-saving package an older one would refuse (docs/decisions.md §33).
pub const CLAUDE_SHARED_CACHE_SINCE: &str = "2.1.306";

/// Whether `version` is `min` or newer, by its numbers (`2.1.310` ≥
/// `2.1.306`; a pre-release counts as its numbers). `false` when either is
/// not a version.
pub fn version_at_least(version: &str, min: &str) -> bool {
    let numbers = |v: &str| -> Option<Vec<u64>> {
        v.split(['-', '+'])
            .next()?
            .split('.')
            .map(|p| p.parse().ok())
            .collect()
    };
    match (numbers(version), numbers(min)) {
        (Some(v), Some(m)) => v >= m,
        _ => false,
    }
}

/// `1.2`, `1.2.3`, `1.2.3-beta.1`, `1.2.3+build`.
fn looks_like_version(word: &str) -> bool {
    let core = word.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() >= 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_by_their_numbers() {
        assert!(version_at_least("2.1.306", "2.1.306"));
        assert!(version_at_least("2.1.310", "2.1.306"));
        assert!(version_at_least("2.10.0", "2.9.9"));
        assert!(version_at_least("3.0", "2.1.306"));
        assert!(!version_at_least("2.1.99", "2.1.306"));
        assert!(!version_at_least("abc", "2.1.306"));
    }

    #[test]
    fn finds_the_version_in_each_cli_format() {
        for (output, version) in [
            ("2.1.306 (Claude Code)\n", "2.1.306"),
            ("codex-cli 0.169.0", "0.169.0"),
            ("0.68.0\n", "0.68.0"),
            ("kimi, version 1.2.0", "1.2.0"),
            ("agy v1.4.2-beta.1", "1.4.2-beta.1"),
            ("Antigravity 1.20 (build 77)", "1.20"),
            ("version 3.0.1.", "3.0.1"),
        ] {
            assert_eq!(parse_version(output).as_deref(), Some(version), "{output}");
        }
    }

    #[test]
    fn ignores_text_without_a_version() {
        for output in ["", "error: unknown flag", "build 77", "1.x", "v.", "a.b"] {
            assert_eq!(parse_version(output), None, "{output}");
        }
    }
}
