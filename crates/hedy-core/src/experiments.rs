//! Behavior that can be switched, to measure it with evals before it becomes
//! Hedy's own (docs/decisions.md §33). New ideas start off; the ones the evals
//! showed are worth it and the user approved are on by default
//! ([`Experiments::DEFAULT`]).
//!
//! One process-wide set, like the language ([`crate::i18n`]): read once from
//! the `HEDY_EXPERIMENTS` environment variable (comma-separated names, e.g.
//! `HEDY_EXPERIMENTS=default,read-guard`, or `none` for Hedy without any of
//! them), or set by a driver such as the eval runner ([`set`]). Hedy hands the
//! same value to every process it starts (the CLIs, their hooks, the memory
//! MCP server), so they all agree.

use std::sync::atomic::{AtomicU16, Ordering};

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
    /// Claude's `PreToolUse` hook denies re-reading a file that has not
    /// changed in the session, and reading lockfiles, minified files and big
    /// files whole (`domain::read_guard`).
    ReadGuard,
    /// The first prompt of a project task ends with a short briefing Hedy
    /// builds without AI: files that match the request, the notes that match
    /// it and the test command (`domain::briefing`).
    Briefing,
    /// Claude Code's short system prompt, without its git instructions and
    /// its own auto memory (environment variables).
    LeanPrompt,
    /// Claude Code with only the built-in tools a Hedy task uses (`--tools`):
    /// no cron, worktree, workflow, messaging or web tools in project tasks.
    LeanTools,
    /// Claude Code's prompt cache for 5 minutes instead of an hour (cheaper
    /// writes) and the per-machine parts of its system prompt moved out of
    /// the cached prefix.
    Cache5m,
    /// Claude Code's subagents (Explore, Plan…) on haiku, whatever the
    /// session's model (on a subscription Explore otherwise runs on Opus).
    ExploreHaiku,
}

impl Experiment {
    pub const ALL: [Experiment; 10] = [
        Self::TrimGuidance,
        Self::TrimTools,
        Self::Search8,
        Self::RankUsage,
        Self::ReadGuard,
        Self::Briefing,
        Self::LeanPrompt,
        Self::LeanTools,
        Self::Cache5m,
        Self::ExploreHaiku,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::TrimGuidance => "trim-guidance",
            Self::TrimTools => "trim-tools",
            Self::Search8 => "search-8",
            Self::RankUsage => "rank-usage",
            Self::ReadGuard => "read-guard",
            Self::Briefing => "briefing",
            Self::LeanPrompt => "lean-prompt",
            Self::LeanTools => "lean-tools",
            Self::Cache5m => "cache-5m",
            Self::ExploreHaiku => "explore-haiku",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|e| e.name() == name.trim().to_lowercase())
    }

    const fn bit(self) -> u16 {
        1 << (self as u8)
    }
}

/// A set of experiments.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Experiments(u16);

impl Experiments {
    /// Hedy without any of them (before Nov 17, 2026).
    pub const NONE: Experiments = Experiments(0);

    /// What Hedy runs with unless told otherwise: the package confirmed on
    /// haiku, Sonnet and Opus (−30 to −41 % cost at the same pass rate,
    /// docs/decisions.md §33).
    pub const DEFAULT: Experiments = Experiments(
        Experiment::TrimGuidance.bit()
            | Experiment::TrimTools.bit()
            | Experiment::Search8.bit()
            | Experiment::RankUsage.bit()
            | Experiment::Briefing.bit()
            | Experiment::LeanPrompt.bit()
            | Experiment::Cache5m.bit(),
    );

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

    /// `"trim-guidance,search-8"`; `"none"` for none. [`Self::parse`] reads
    /// it back.
    pub fn names(self) -> String {
        if self.is_empty() {
            return "none".into();
        }
        self.iter()
            .map(Experiment::name)
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Names separated by commas or `+`, added up: `default` (or `base`) for
    /// [`Self::DEFAULT`], `all` for every one, `none` for nothing. Unknown
    /// names come back apart.
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
                "default" | "base" => set = Self(set.0 | Self::DEFAULT.0),
                "none" => {}
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
const READY: u16 = 0x8000;
static CURRENT: AtomicU16 = AtomicU16::new(0);

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
    // Tests never read the developer's environment, and start from nothing:
    // each one turns on what it checks.
    let set = if cfg!(test) {
        Experiments::NONE
    } else {
        from_environment()
    };
    // Only if nobody got there first: a driver's `set` on another thread
    // wins over the environment.
    match CURRENT.compare_exchange(value, set.0 | READY, Ordering::Relaxed, Ordering::Relaxed) {
        Ok(_) => set,
        Err(now) => Experiments(now & !READY),
    }
}

/// Whether `e` is on in this process.
pub fn on(e: Experiment) -> bool {
    current().has(e)
}

/// Replaces the process's experiments (a driver such as the eval runner, or
/// the user's "Ahorro" setting). In tests it changes this thread's only.
pub fn set(set: Experiments) {
    #[cfg(test)]
    OVERRIDE.with(|o| o.set(Some(set)));
    #[cfg(not(test))]
    CURRENT.store(set.0 | READY, Ordering::Relaxed);
}

/// Whether `HEDY_EXPERIMENTS` is set: then it decides, over the setting.
pub fn environment_decides() -> bool {
    !cfg!(test) && std::env::var_os(ENV_VAR).is_some()
}

/// What `HEDY_EXPERIMENTS` asks for ([`Experiments::DEFAULT`] when it is
/// not set); unknown names are ignored.
pub fn from_environment() -> Experiments {
    std::env::var(ENV_VAR)
        .map(|v| Experiments::parse(&v).0)
        .unwrap_or(Experiments::DEFAULT)
}

/// The variable to hand to a child process (the CLIs, their hooks, the memory
/// MCP server) so it runs with the same experiments, `none` included.
pub fn child_env() -> (String, String) {
    (ENV_VAR.to_owned(), current().names())
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
        assert!(Experiments::parse("").0.is_empty());
    }

    #[test]
    fn the_default_is_the_confirmed_package() {
        let names = Experiments::DEFAULT.names();
        assert_eq!(
            names,
            "trim-guidance,trim-tools,search-8,rank-usage,briefing,lean-prompt,cache-5m"
        );
        assert_eq!(Experiments::parse("default").0, Experiments::DEFAULT);
        assert_eq!(Experiments::parse("base").0, Experiments::DEFAULT);
        let more = Experiments::parse("default,read-guard").0;
        assert!(more.has(Experiment::ReadGuard) && more.has(Experiment::Cache5m));
        assert_eq!(Experiments::NONE.names(), "none");
        assert!(Experiments::parse("none").0.is_empty());
    }

    #[test]
    fn tests_start_from_none_and_override_per_thread() {
        assert!(current().is_empty());
        assert_eq!(child_env(), (ENV_VAR.into(), "none".into()));
        let seen = with_experiments(Experiments::DEFAULT, || {
            (on(Experiment::Search8), child_env())
        });
        assert!(seen.0);
        assert_eq!(seen.1.1, Experiments::DEFAULT.names());
        assert!(current().is_empty());
    }
}
