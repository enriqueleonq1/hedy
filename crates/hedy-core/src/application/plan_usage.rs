//! How much of each subscription plan is used, from each provider's own
//! source (docs/decisions.md §20). Readings are cached per
//! account so screens show the last one at once; a new one is asked at most
//! every [`PLAN_USAGE_MIN_INTERVAL_SECS`].

use std::time::Duration;

use super::Setting;
use super::ports::Secret;
use super::ports::{PlanUsageRecord, RateLimit};
use super::{AppResult, Hedy};
use crate::domain::plan_usage::{
    ClaudeUsageAnswer, moonshot_currency, parse_claude_usage, parse_codex_rate_limits,
    parse_moonshot_balance, resolve_windows,
};
use crate::domain::version::parse_version;
use crate::domain::{
    Account, LaunchPurpose, LaunchSpec, LimitLevel, PlanUsage, Provider, UsageProblem, UsageSource,
    UsageWindow, WindowKind,
};

/// A new reading is asked at most this often per account.
pub const PLAN_USAGE_MIN_INTERVAL_SECS: i64 = 180;

/// `/usage` took about 6–8 s in real runs: it starts Claude Code.
const CLAUDE_USAGE_TIMEOUT: Duration = Duration::from_secs(30);
const CODEX_USAGE_TIMEOUT: Duration = Duration::from_secs(20);
const BALANCE_TIMEOUT: Duration = Duration::from_secs(15);
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// Internal state, not a user setting: the CLI version whose usage command
/// reached the model. Hedy does not ask that version again.
fn spends_tokens_key(provider: Provider) -> String {
    format!("plan_usage.spends_tokens.{}", provider.as_str())
}

/// An account close to or at its limit, from what Hedy already knows (the
/// cached reading and the last report of a task), never asking the provider.
#[derive(Debug, Clone, PartialEq)]
pub struct LimitCheck {
    pub level: LimitLevel,
    /// The window, when the plan reading says which one.
    pub window: Option<UsageWindow>,
    /// When it renews (Unix seconds), when known.
    pub resets_at: Option<i64>,
    /// When the figure was read (Unix seconds).
    pub checked_at: i64,
}

/// What screens show about an account's plan usage.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanUsageState {
    /// The last known reading, merged with fresher reports from Hedy's tasks.
    pub usage: Option<PlanUsage>,
    /// When Hedy last asked (Unix seconds).
    pub attempted_at: Option<i64>,
    /// Why the last attempt gave nothing.
    pub problem: Option<UsageProblem>,
    /// Whether Hedy has a source for this account at all.
    pub supported: bool,
}

impl Hedy {
    /// Whether Hedy has a usage source for this account: Claude Code and
    /// Codex subscriptions, and Kimi API keys on Moonshot's own platform.
    pub fn plan_usage_supported(&self, account: &Account) -> AppResult<bool> {
        Ok(match (account.provider, account.is_subscription()) {
            (Provider::Claude | Provider::Codex, true) => true,
            (Provider::Kimi, false) => moonshot_currency(&self.api_base_url(account)?).is_some(),
            _ => false,
        })
    }

    fn api_base_url(&self, account: &Account) -> AppResult<String> {
        Ok(self
            .setting(Setting::ApiUrl(account.provider))?
            .or_else(|| account.provider.api_base_url().map(str::to_owned))
            .unwrap_or_default())
    }

    /// The cached reading, without starting anything.
    pub fn plan_usage(&self, id: &str) -> AppResult<PlanUsageState> {
        let account = self.account(id)?;
        self.plan_usage_of(&account)
    }

    pub(crate) fn plan_usage_of(&self, account: &Account) -> AppResult<PlanUsageState> {
        let supported = self.plan_usage_supported(account)?;
        let record = self.store.plan_usage(&account.id)?;
        let limit = self.store.rate_limit(&account.id)?;
        let (usage, attempted_at, problem) = match record {
            Some(r) => (r.usage, Some(r.attempted_at), r.problem),
            None => (None, None, None),
        };
        Ok(PlanUsageState {
            usage: merge_task_report(usage, limit.as_ref()),
            attempted_at,
            problem: if supported {
                problem
            } else {
                Some(UsageProblem::Unsupported)
            },
            supported,
        })
    }

    /// Whether `account` is close to (≥ 90 %) or at its limit, from the
    /// cache only, so it never slows a launch down: the window closest to
    /// its limit ([`PlanUsage::limit_alert`]), else a task's last report
    /// that it was rejected and has not renewed yet.
    pub fn limit_check(&self, account: &Account) -> AppResult<Option<LimitCheck>> {
        let now = self.now_secs();
        let usage = self.plan_usage_of(account)?.usage;
        if let Some(usage) = &usage
            && let Some(alert) = usage.limit_alert(now)
        {
            return Ok(Some(LimitCheck {
                level: alert.level,
                resets_at: alert.window.resets_at,
                window: Some(alert.window),
                checked_at: usage.checked_at,
            }));
        }
        let Some(limit) = self.store.rate_limit(&account.id)? else {
            return Ok(None);
        };
        let level = LimitLevel::from_report(&limit.status, limit.utilization);
        let current = limit.resets_at.map_or(now - limit.at < 3600, |at| at > now);
        // A newer reading that says nothing is wrong wins over an old report.
        let superseded = usage.is_some_and(|u| u.checked_at >= limit.at);
        let reached = level == Some(LimitLevel::Reached) && current && !superseded;
        Ok(reached.then_some(LimitCheck {
            level: LimitLevel::Reached,
            window: None,
            resets_at: limit.resets_at,
            checked_at: limit.at,
        }))
    }

    /// Asks the provider for a new reading unless one was asked less than
    /// [`PLAN_USAGE_MIN_INTERVAL_SECS`] ago. Failures keep the last reading.
    pub fn refresh_plan_usage(&self, id: &str) -> AppResult<PlanUsageState> {
        let account = self.account(id)?;
        if !self.plan_usage_supported(&account)? {
            return self.plan_usage_of(&account);
        }
        let now = self.clock.now_ms() / 1000;
        let previous = self.store.plan_usage(&account.id)?;
        if previous
            .as_ref()
            .is_some_and(|r| now - r.attempted_at < PLAN_USAGE_MIN_INTERVAL_SECS)
        {
            return self.plan_usage_of(&account);
        }
        let last = previous.and_then(|r| r.usage);
        // Claimed before asking, so a second caller (another screen, the CLI)
        // does not start the same command meanwhile.
        self.store.set_plan_usage(
            &account.id,
            &PlanUsageRecord {
                usage: last.clone(),
                attempted_at: now,
                problem: None,
            },
        )?;
        let (usage, problem) = match self.read_plan_usage(&account, now) {
            Ok(usage) => (Some(usage), None),
            Err(problem) => (last, Some(problem)),
        };
        self.store.set_plan_usage(
            &account.id,
            &PlanUsageRecord {
                usage,
                attempted_at: now,
                problem,
            },
        )?;
        self.plan_usage_of(&account)
    }

    fn read_plan_usage(&self, account: &Account, now: i64) -> Result<PlanUsage, UsageProblem> {
        if !account.is_subscription() {
            return self.read_api_balance(account, now);
        }
        let program = self
            .binary_for(account.provider)
            .map_err(|_| UsageProblem::Unavailable)?;
        let Some(spec) =
            LaunchSpec::for_account(account, LaunchPurpose::PlanUsage, program.as_deref())
                .map_err(|_| UsageProblem::Unsupported)?
        else {
            return Err(UsageProblem::Unsupported);
        };
        match account.provider {
            Provider::Claude => self.read_claude_usage(&spec, now),
            Provider::Codex => {
                let json = self
                    .usage
                    .codex_rate_limits(&spec, CODEX_USAGE_TIMEOUT)
                    .map_err(|_| UsageProblem::Unavailable)?;
                parse_codex_rate_limits(&json, now).ok_or(UsageProblem::Unavailable)
            }
            _ => Err(UsageProblem::Unsupported),
        }
    }

    /// Moonshot's balance endpoint, with the key from the keyring.
    fn read_api_balance(&self, account: &Account, now: i64) -> Result<PlanUsage, UsageProblem> {
        let base_url = self
            .api_base_url(account)
            .map_err(|_| UsageProblem::Unavailable)?;
        let currency = moonshot_currency(&base_url).ok_or(UsageProblem::Unsupported)?;
        let key = self
            .secrets
            .api_key(&account.id)
            .ok()
            .flatten()
            .ok_or(UsageProblem::Unavailable)?;
        let body = self
            .usage
            .moonshot_balance(&base_url, &Secret::new(key), BALANCE_TIMEOUT)
            .map_err(|_| UsageProblem::Unavailable)?;
        parse_moonshot_balance(&body, currency, now).ok_or(UsageProblem::Unavailable)
    }

    /// `claude -p /usage --output-format json`. If an answer ever shows a
    /// model call (turns, cost or tokens above zero), that version is
    /// remembered and never asked again.
    fn read_claude_usage(&self, spec: &LaunchSpec, now: i64) -> Result<PlanUsage, UsageProblem> {
        let key = spends_tokens_key(Provider::Claude);
        let banned = self
            .store
            .get_setting(&key)
            .map_err(|_| UsageProblem::Unavailable)?;
        if let Some(banned) = banned {
            match self.cli_version(&spec.program) {
                Some(version) if version == banned => return Err(UsageProblem::SpendsTokens),
                // Unknown version: do not risk it.
                None => return Err(UsageProblem::SpendsTokens),
                Some(_) => {
                    let _ = self.store.set_setting(&key, None);
                }
            }
        }
        let out = self
            .runner
            .run_captured_within(spec, CLAUDE_USAGE_TIMEOUT)
            .map_err(|_| UsageProblem::Unavailable)?;
        match parse_claude_usage(&out.stdout) {
            ClaudeUsageAnswer::Windows(windows) => Ok(resolve_windows(
                windows,
                now,
                UsageSource::ClaudeUsage,
                |wall| self.clock.resolve_wall_time(wall, now),
            )),
            ClaudeUsageAnswer::SpentTokens => {
                let version = self
                    .cli_version(&spec.program)
                    .unwrap_or_else(|| "unknown".into());
                let _ = self.store.set_setting(&key, Some(&version));
                Err(UsageProblem::SpendsTokens)
            }
            ClaudeUsageAnswer::NoData => Err(UsageProblem::Unavailable),
        }
    }

    fn cli_version(&self, program: &str) -> Option<String> {
        let spec = LaunchSpec::program(program, vec!["--version".into()]);
        self.runner
            .run_captured_within(&spec, VERSION_TIMEOUT)
            .ok()
            .filter(|out| out.exit_code == Some(0))
            .and_then(|out| parse_version(&out.stdout))
    }
}

/// Folds in a rate-limit report from one of Hedy's tasks when it is newer
/// than the reading and carries a percentage:
/// - it updates the window that renews at the same time (± 1 hour);
/// - with no reading at all, it becomes one window, a session when it renews
///   within about 5 hours of the report, otherwise a week.
///
/// A report without a percentage (Claude on Team plans) changes nothing.
fn merge_task_report(usage: Option<PlanUsage>, limit: Option<&RateLimit>) -> Option<PlanUsage> {
    let Some(limit) = limit else {
        return usage;
    };
    let Some(percent) = limit.utilization else {
        return usage;
    };
    if usage.as_ref().is_some_and(|u| u.checked_at >= limit.at) {
        return usage;
    }
    match usage {
        Some(mut usage) => {
            let same = limit.resets_at.and_then(|resets| {
                usage.windows.iter_mut().find(|w| {
                    w.model.is_none() && w.resets_at.is_some_and(|r| (r - resets).abs() <= 3600)
                })
            });
            if let Some(window) = same {
                window.percent = percent;
                usage.checked_at = limit.at;
                usage.source = UsageSource::TaskReport;
            }
            Some(usage)
        }
        None => {
            let kind = match limit.resets_at {
                Some(resets) if resets - limit.at <= 5 * 3600 + 600 => WindowKind::Session,
                Some(_) => WindowKind::Week,
                None => return None,
            };
            Some(PlanUsage {
                windows: vec![UsageWindow {
                    kind,
                    minutes: None,
                    model: None,
                    percent,
                    resets_text: None,
                    resets_at: limit.resets_at,
                }],
                checked_at: limit.at,
                source: UsageSource::TaskReport,
                balance: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::Setting;
    use crate::application::ports::{PortError, SettingsRepository};
    use crate::application::testing::Fixture;
    use crate::domain::AccountId;

    const SAMPLE_TEXT: &str = "You are currently using your subscription to power your Claude Code usage\n\nCurrent session: 34% used · resets Nov 14, 7pm (America/Caracas)\nCurrent week (all models): 29% used · resets Nov 20, 12am (America/Caracas)\nCurrent week (Fable): 0% used · resets Nov 20, 12am (America/Caracas)\n";

    fn answer(turns: u32, cost: f64) -> String {
        serde_json::json!({"type": "result", "is_error": false, "num_turns": turns,
            "total_cost_usd": cost, "result": SAMPLE_TEXT})
        .to_string()
    }

    const USAGE_CMD: &str =
        "claude -p /usage --output-format json --no-session-persistence --safe-mode";

    #[test]
    fn reads_claude_usage_and_caches_it() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.runner.reply_to(USAGE_CMD, &answer(0, 0.0), 0);

        let state = f.hedy.refresh_plan_usage("personal").unwrap();
        assert!(state.supported);
        assert_eq!(state.problem, None);
        let usage = state.usage.unwrap();
        assert_eq!(usage.source, UsageSource::ClaudeUsage);
        assert_eq!(usage.session().unwrap().percent, 34.0);
        assert_eq!(usage.week().unwrap().percent, 29.0);
        // The fake clock resolves zoned times to an hour after the check.
        assert_eq!(
            usage.session().unwrap().resets_at,
            Some(usage.checked_at + 3600)
        );
        let spec = f.runner.captured().pop().unwrap();
        assert_eq!(
            spec.env,
            vec![("CLAUDE_CONFIG_DIR".into(), "/data/accounts/personal".into())]
        );

        // The cache answers without starting anything.
        let runs = f.runner.captured().len();
        assert_eq!(
            f.hedy.plan_usage("personal").unwrap().usage,
            Some(usage.clone())
        );
        // Within the interval, a refresh does not ask again.
        f.hedy.refresh_plan_usage("personal").unwrap();
        assert_eq!(f.runner.captured().len(), runs);
        f.clock.advance(PLAN_USAGE_MIN_INTERVAL_SECS);
        f.hedy.refresh_plan_usage("personal").unwrap();
        assert_eq!(f.runner.captured().len(), runs + 1);
    }

    #[test]
    fn failures_keep_the_last_reading() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.runner.reply_to(USAGE_CMD, &answer(0, 0.0), 0);
        let first = f.hedy.refresh_plan_usage("personal").unwrap().usage;

        f.runner.fail(USAGE_CMD);
        f.clock.advance(PLAN_USAGE_MIN_INTERVAL_SECS);
        let state = f.hedy.refresh_plan_usage("personal").unwrap();
        assert_eq!(state.problem, Some(UsageProblem::Unavailable));
        assert_eq!(state.usage, first);

        f.runner.reply_to(USAGE_CMD, "Unknown command", 0);
        f.clock.advance(PLAN_USAGE_MIN_INTERVAL_SECS);
        let state = f.hedy.refresh_plan_usage("personal").unwrap();
        assert_eq!(state.problem, Some(UsageProblem::Unavailable));
        assert_eq!(state.usage, first);
    }

    #[test]
    fn a_version_that_spends_tokens_is_not_asked_again() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.runner
            .reply_to("claude --version", "2.1.400 (Claude Code)", 0);
        f.runner.reply_to(USAGE_CMD, &answer(1, 0.01), 0);
        let state = f.hedy.refresh_plan_usage("personal").unwrap();
        assert_eq!(state.problem, Some(UsageProblem::SpendsTokens));
        assert_eq!(state.usage, None);
        assert_eq!(
            f.store
                .get_setting("plan_usage.spends_tokens.claude")
                .unwrap()
                .as_deref(),
            Some("2.1.400")
        );

        let asked = |f: &Fixture| {
            f.runner
                .captured()
                .iter()
                .filter(|s| s.args.contains(&"/usage".to_owned()))
                .count()
        };
        f.clock.advance(PLAN_USAGE_MIN_INTERVAL_SECS);
        f.hedy.refresh_plan_usage("personal").unwrap();
        assert_eq!(asked(&f), 1, "the same version is not asked again");

        // A new version is tried again.
        f.runner
            .reply_to("claude --version", "2.1.401 (Claude Code)", 0);
        f.runner.reply_to(USAGE_CMD, &answer(0, 0.0), 0);
        f.clock.advance(PLAN_USAGE_MIN_INTERVAL_SECS);
        let state = f.hedy.refresh_plan_usage("personal").unwrap();
        assert_eq!(asked(&f), 2);
        assert_eq!(state.problem, None);
        assert!(state.usage.is_some());
    }

    #[test]
    fn respects_the_binary_setting() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.hedy
            .set_setting(Setting::Binary(Provider::Claude), Some("/opt/claude"))
            .unwrap();
        f.runner.fail("x");
        let _ = f.hedy.refresh_plan_usage("personal").unwrap();
        assert_eq!(f.runner.captured().pop().unwrap().program, "/opt/claude");
    }

    #[test]
    fn other_providers_are_not_supported_yet() {
        let f = Fixture::new();
        f.add("Gemini", Provider::Gemini, false);
        f.add("Agy", Provider::Antigravity, false);
        f.add("Kimi Code", Provider::Kimi, false);
        for id in ["gemini", "agy", "kimi-code"] {
            let state = f.hedy.refresh_plan_usage(id).unwrap();
            assert!(!state.supported, "{id}");
            assert_eq!(state.problem, Some(UsageProblem::Unsupported));
        }
        assert!(f.runner.captured().is_empty());
    }

    #[test]
    fn kimi_api_keys_read_the_moonshot_balance() {
        let f = Fixture::new();
        f.add_kimi("Kimi");
        f.hedy.set_api_key("kimi", "sk-1").unwrap();
        f.probe.reply_balance(Ok(
            r#"{"code":0,"data":{"available_balance":12.5},"status":true}"#.into(),
        ));
        let state = f.hedy.refresh_plan_usage("kimi").unwrap();
        assert!(state.supported);
        let balance = state.usage.unwrap().balance.unwrap();
        assert_eq!(
            (balance.available, balance.currency.as_str()),
            (12.5, "USD")
        );
        assert_eq!(
            f.probe.balance_calls(),
            [("https://api.moonshot.ai/v1".to_owned(), "sk-1".to_owned())]
        );

        // Kimi Code's endpoint has no documented balance.
        f.hedy
            .set_setting(
                Setting::ApiUrl(Provider::Kimi),
                Some("https://api.kimi.com/coding/v1"),
            )
            .unwrap();
        assert!(!f.hedy.plan_usage("kimi").unwrap().supported);
    }

    #[test]
    fn reads_codex_through_the_probe() {
        let f = Fixture::new();
        f.add_codex("Codex");
        f.probe.reply(Ok(serde_json::json!({"rateLimits": {
            "primary": {"usedPercent": 42, "windowDurationMins": 300, "resetsAt": 1_700_003_600},
            "secondary": {"usedPercent": 7, "windowDurationMins": 10080, "resetsAt": 1_700_500_000}
        }})
        .to_string()));
        let usage = f.hedy.refresh_plan_usage("codex").unwrap().usage.unwrap();
        assert_eq!(usage.source, UsageSource::CodexRateLimits);
        assert_eq!(usage.session().unwrap().percent, 42.0);
        assert_eq!(usage.week().unwrap().resets_at, Some(1_700_500_000));
        let spec = f.probe.calls().pop().unwrap();
        assert_eq!(spec.args, ["app-server", "--listen", "stdio://"]);
        assert_eq!(spec.env[0].0, "CODEX_HOME");

        f.probe
            .reply(Err(PortError::new("authentication required")));
        f.clock.advance(PLAN_USAGE_MIN_INTERVAL_SECS);
        let state = f.hedy.refresh_plan_usage("codex").unwrap();
        assert_eq!(state.problem, Some(UsageProblem::Unavailable));
        assert_eq!(state.usage, Some(usage));
    }

    fn limit(utilization: Option<f64>, resets_at: Option<i64>, at: i64) -> RateLimit {
        RateLimit {
            status: "allowed".into(),
            utilization,
            resets_at,
            at,
        }
    }

    #[test]
    fn fresher_task_reports_update_the_matching_window() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.runner.reply_to(USAGE_CMD, &answer(0, 0.0), 0);
        let usage = f
            .hedy
            .refresh_plan_usage("personal")
            .unwrap()
            .usage
            .unwrap();
        let session_reset = usage.session().unwrap().resets_at.unwrap();
        let id = AccountId::new("personal").unwrap();

        // Older than the reading: ignored.
        use crate::application::ports::UsageRepository;
        f.store
            .set_rate_limit(&id, &limit(Some(90.0), Some(session_reset), 1))
            .unwrap();
        assert_eq!(
            f.hedy.plan_usage("personal").unwrap().usage,
            Some(usage.clone())
        );

        // Newer, same reset time: it updates the session.
        let at = usage.checked_at + 60;
        f.store
            .set_rate_limit(&id, &limit(Some(51.0), Some(session_reset + 30), at))
            .unwrap();
        let merged = f.hedy.plan_usage("personal").unwrap().usage.unwrap();
        assert_eq!(merged.session().unwrap().percent, 51.0);
        assert_eq!(merged.week().unwrap().percent, 29.0);
        assert_eq!(merged.source, UsageSource::TaskReport);
        assert_eq!(merged.checked_at, at);

        // Without a percentage (Team plans), nothing changes.
        f.store
            .set_rate_limit(&id, &limit(None, Some(session_reset), at + 10))
            .unwrap();
        assert_eq!(f.hedy.plan_usage("personal").unwrap().usage, Some(usage));
    }

    #[test]
    fn a_task_report_alone_becomes_one_window() {
        let report = limit(Some(70.0), Some(10_000 + 3 * 3600), 10_000);
        let usage = merge_task_report(None, Some(&report)).unwrap();
        assert_eq!(usage.windows.len(), 1);
        assert_eq!(usage.session().unwrap().percent, 70.0);
        let weekly = limit(Some(70.0), Some(10_000 + 3 * 86_400), 10_000);
        assert!(
            merge_task_report(None, Some(&weekly))
                .unwrap()
                .week()
                .is_some()
        );
        assert_eq!(
            merge_task_report(None, Some(&limit(Some(5.0), None, 1))),
            None
        );
    }
}
