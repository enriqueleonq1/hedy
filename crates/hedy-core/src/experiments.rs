//! Experimental behavior, off by default until evals measure it and the user
//! sees the results (docs/decisions.md §33).
//!
//! One process-wide set, like the language ([`crate::i18n`]): read once from
//! the `HEDY_EXPERIMENTS` environment variable (comma-separated names, e.g.
//! `HEDY_EXPERIMENTS=trim-guidance,search-8`), or set by a driver such as the
//! eval runner ([`set`]). Hedy hands the same value to the memory MCP server
//! it starts, so both processes agree.

use std::sync::atomic::{AtomicU8, Ordering};

/// The environment variable that turns experiments on.
pub const ENV_VAR: &str = "HEDY_EXPERIMENTS";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Experiment {
    /// The shorter `--append-system-prompt` guidance (`prompts::memory_guidance`).
    TrimGuidance,
    /// The shorter, English memory and skill tool definitions.
    TrimTools,
    /// `memory_search` returns 8 notes (not 30) and `skill_list` 20, with
    /// `limit` / `offset` and a line saying how to ask for more.
    Search8,
    /// Search results ranked with this project's note uses, notes "para
    /// revisar" last (`domain::recall::rank_notes`).
    RankUsage,
}

impl Experiment {
    pub const ALL: [Experiment; 4] = [
        Self::TrimGuidance,
        Self::TrimTools,
        Self::Search8,
        Self::RankUsage,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::TrimGuidance => "trim-guidance",
            Self::TrimTools => "trim-tools",
            Self::Search8 => "search-8",
            Self::RankUsage => "rank-usage",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|e| e.name() == name.trim().to_lowercase())
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// A set of experiments. Empty: today's behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Experiments(u8);

impl Experiments {
    pub const NONE: Experiments = Experiments(0);

    pub fn all() -> Self {
        Experiment::ALL.into_iter().collect()
    }

    pub fn has(self, e: Experiment) -> bool {
        self.0 & e.bit() != 0
    }

    pub fn with(self, e: Experiment) -> Self {
        Self(self.0 | e.bit())
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Experiment> {
        Experiment::ALL.into_iter().filter(move |e| self.has(*e))
    }

    /// `"trim-guidance,search-8"`; empty for none. [`Self::parse`] reads it back.
    pub fn names(self) -> String {
        self.iter()
            .map(Experiment::name)
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Names separated by commas or `+` (`all` for every one, `base` or
    /// `none` for none). Unknown names come back apart.
    pub fn parse(text: &str) -> (Self, Vec<String>) {
        let mut set = Self::NONE;
        let mut unknown = Vec::new();
        for name in text
            .split([',', '+'])
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            match name.to_lowercase().as_str() {
                "all" => set = Self::all(),
                "base" | "none" => {}
                _ => match Experiment::from_name(name) {
                    Some(e) => set = set.with(e),
                    None => unknown.push(name.to_owned()),
                },
            }
        }
        (set, unknown)
    }
}

impl FromIterator<Experiment> for Experiments {
    fn from_iter<I: IntoIterator<Item = Experiment>>(iter: I) -> Self {
        iter.into_iter().fold(Self::NONE, Self::with)
    }
}

/// High bit: the set was read or given.
const READY: u8 = 0x80;
static CURRENT: AtomicU8 = AtomicU8::new(0);

#[cfg(test)]
thread_local! {
    /// Lets a test turn experiments on without touching other tests' threads.
    static OVERRIDE: std::cell::Cell<Option<Experiments>> = const { std::cell::Cell::new(None) };
}

/// Runs `f` with `set` on this thread only (tests).
#[cfg(test)]
pub fn with_experiments<T>(set: Experiments, f: impl FnOnce() -> T) -> T {
    OVERRIDE.with(|o| o.set(Some(set)));
    let result = f();
    OVERRIDE.with(|o| o.set(None));
    result
}

/// The experiments this process runs with.
pub fn current() -> Experiments {
    #[cfg(test)]
    if let Some(set) = OVERRIDE.with(|o| o.get()) {
        return set;
    }
    let value = CURRENT.load(Ordering::Relaxed);
    if value & READY != 0 {
        return Experiments(value & !READY);
    }
    // Tests never read the developer's environment.
    let set = if cfg!(test) {
        Experiments::NONE
    } else {
        from_environment()
    };
    CURRENT.store(set.0 | READY, Ordering::Relaxed);
    set
}

/// Whether `e` is on in this process.
pub fn on(e: Experiment) -> bool {
    current().has(e)
}

/// Replaces the process's experiments (a driver such as the eval runner).
pub fn set(set: Experiments) {
    CURRENT.store(set.0 | READY, Ordering::Relaxed);
}

/// What `HEDY_EXPERIMENTS` asks for; unknown names are ignored.
pub fn from_environment() -> Experiments {
    std::env::var(ENV_VAR)
        .map(|v| Experiments::parse(&v).0)
        .unwrap_or_default()
}

/// The variable to hand to a child process (the memory MCP server) so it
/// runs with the same experiments; `None` when there are none.
pub fn child_env() -> Option<(String, String)> {
    let set = current();
    (!set.is_empty()).then(|| (ENV_VAR.to_owned(), set.names()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_and_reports_unknown_ones() {
        let (set, unknown) = Experiments::parse(" trim-guidance, search-8 ,bogus");
        assert!(set.has(Experiment::TrimGuidance) && set.has(Experiment::Search8));
        assert!(!set.has(Experiment::RankUsage));
        assert_eq!(unknown, ["bogus"]);
        assert_eq!(set.names(), "trim-guidance,search-8");
        assert_eq!(Experiments::parse(&set.names()).0, set);
        assert_eq!(
            Experiments::parse("trim-tools+rank-usage").0.names(),
            "trim-tools,rank-usage"
        );
        assert_eq!(Experiments::parse("all").0, Experiments::all());
        assert!(Experiments::parse("base").0.is_empty());
        assert!(Experiments::parse("").0.is_empty());
    }

    #[test]
    fn off_by_default_and_overridable_per_thread() {
        assert!(current().is_empty());
        assert_eq!(child_env(), None);
        let seen = with_experiments(Experiments::all(), || {
            (on(Experiment::Search8), child_env())
        });
        assert!(seen.0);
        assert_eq!(
            seen.1,
            Some((
                ENV_VAR.into(),
                "trim-guidance,trim-tools,search-8,rank-usage".into()
            ))
        );
        assert!(current().is_empty());
    }
}
