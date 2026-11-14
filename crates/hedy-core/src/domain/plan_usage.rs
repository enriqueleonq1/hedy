//! How much of a subscription plan an account has used, per limit window
//! (5-hour session, week, day, per-model week), as each provider reports it.
//!
//! Every provider has its own source (Claude Code's `/usage` text, Codex's
//! `account/rateLimits/read`…), and all of them end up in [`PlanUsage`].
//! Parsers are tolerant: anything they do not understand is left out, and an
//! answer with no window at all is "no data", never an error.

use serde::{Deserialize, Serialize};

/// What a limit window covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowKind {
    /// The rolling 5-hour session.
    Session,
    Week,
    Day,
    /// Any other length (see [`UsageWindow::minutes`]).
    Other,
}

impl WindowKind {
    /// From a window length in minutes, as Codex reports it.
    pub fn from_minutes(minutes: u32) -> Self {
        match minutes {
            300 => Self::Session,
            1440 => Self::Day,
            10080 => Self::Week,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub kind: WindowKind,
    /// Length in minutes, when the source says it.
    pub minutes: Option<u32>,
    /// The model this window is limited to; `None`: every model.
    pub model: Option<String>,
    /// 0–100.
    pub percent: f64,
    /// When it renews, as the source wrote it ("Nov 20, 12am (America/Caracas)").
    pub resets_text: Option<String>,
    /// When it renews, in Unix seconds, when it could be worked out.
    pub resets_at: Option<i64>,
}

/// Where the numbers came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageSource {
    /// `claude -p /usage --output-format json`.
    ClaudeUsage,
    /// `codex app-server`, method `account/rateLimits/read`.
    CodexRateLimits,
    /// A rate-limit report during one of Hedy's tasks.
    TaskReport,
    /// Moonshot's `GET /v1/users/me/balance` (Kimi API keys).
    MoonshotBalance,
}

/// Prepaid API credit (Kimi API keys), as the provider reports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    /// What can still be spent (cash plus vouchers).
    pub available: f64,
    /// ISO 4217: `USD` on the global platform, `CNY` on the China one.
    pub currency: String,
}

/// Why the last attempt to read the usage gave nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageProblem {
    /// This provider (or kind of account) has no source Hedy can read yet.
    Unsupported,
    /// The CLI did not answer, failed, or answered something unknown.
    Unavailable,
    /// This CLI version reached the model to answer, so Hedy stopped asking it
    /// until the version changes.
    SpendsTokens,
}

impl UsageProblem {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Unavailable => "unavailable",
            Self::SpendsTokens => "spends_tokens",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Unsupported, Self::Unavailable, Self::SpendsTokens]
            .into_iter()
            .find(|p| p.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanUsage {
    pub windows: Vec<UsageWindow>,
    /// Unix seconds.
    pub checked_at: i64,
    pub source: UsageSource,
    /// Prepaid credit, for API keys whose provider reports it.
    #[serde(default)]
    pub balance: Option<Balance>,
}

impl PlanUsage {
    fn shared(&self, kind: WindowKind) -> Option<&UsageWindow> {
        self.windows
            .iter()
            .find(|w| w.kind == kind && w.model.is_none())
    }

    /// The 5-hour session window (every model).
    pub fn session(&self) -> Option<&UsageWindow> {
        self.shared(WindowKind::Session)
    }

    /// The weekly window for every model.
    pub fn week(&self) -> Option<&UsageWindow> {
        self.shared(WindowKind::Week)
    }

    /// Windows limited to one model.
    pub fn model_windows(&self) -> impl Iterator<Item = &UsageWindow> {
        self.windows.iter().filter(|w| w.model.is_some())
    }

    /// The figure for a compact label: the window shared by every model that
    /// is closest to its limit, since that is the one that stops the work
    /// first (a 10 % session does not help when the week is at 97 %).
    pub fn headline(&self) -> Option<&UsageWindow> {
        self.windows
            .iter()
            .filter(|w| w.model.is_none())
            .max_by(|a, b| a.percent.total_cmp(&b.percent))
    }
}

/// A time of day in a time zone, as `/usage` writes resets
/// ("Nov 14, 7pm (America/Caracas)"). Turning it into an instant needs the
/// time-zone database, which is an adapter's job ([`crate::application::ports::Clock`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallTime {
    /// 1–12, with `day`; both `None` when only the time is given.
    pub month: Option<u8>,
    pub day: Option<u8>,
    /// 0–23.
    pub hour: u8,
    pub minute: u8,
    /// IANA name; `None`: the local zone.
    pub zone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reset {
    At(WallTime),
    /// "resets in 3 hr": seconds from the check.
    In(i64),
}

/// A window as read from `/usage`, before resolving its reset time.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedWindow {
    pub kind: WindowKind,
    pub model: Option<String>,
    pub percent: f64,
    pub resets_text: Option<String>,
    pub reset: Option<Reset>,
}

/// What `claude -p /usage --output-format json` answered.
#[derive(Debug, Clone, PartialEq)]
pub enum ClaudeUsageAnswer {
    Windows(Vec<ParsedWindow>),
    /// The command reached the model (turns or cost above zero): this version
    /// does not run `/usage` locally, so Hedy must stop asking it.
    SpentTokens,
    /// Not the expected JSON, or no window in the text.
    NoData,
}

/// Reads the JSON result of `claude -p /usage --output-format json`.
pub fn parse_claude_usage(stdout: &str) -> ClaudeUsageAnswer {
    let Some(json) = stdout
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .or_else(|| serde_json::from_str(stdout.trim()).ok())
    else {
        return ClaudeUsageAnswer::NoData;
    };
    let turns = json["num_turns"].as_f64().unwrap_or(0.0);
    let cost = json["total_cost_usd"].as_f64().unwrap_or(0.0);
    let usage = &json["usage"];
    let tokens: f64 = [
        "input_tokens",
        "output_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
    ]
    .iter()
    .filter_map(|k| usage[k].as_f64())
    .sum();
    if turns > 0.0 || cost > 0.0 || tokens > 0.0 {
        return ClaudeUsageAnswer::SpentTokens;
    }
    if json["is_error"].as_bool() == Some(true) {
        return ClaudeUsageAnswer::NoData;
    }
    let windows = parse_usage_text(json["result"].as_str().unwrap_or_default());
    if windows.is_empty() {
        ClaudeUsageAnswer::NoData
    } else {
        ClaudeUsageAnswer::Windows(windows)
    }
}

/// The limit lines of `/usage`:
///
/// ```text
/// Current session: 34% used · resets Nov 14, 7pm (America/Caracas)
/// Current week (all models): 29% used · resets Nov 20, 12am (America/Caracas)
/// Current week (Fable): 0% used · resets Nov 20, 12am (America/Caracas)
/// ```
///
/// A line counts when the text before its percentage names a window
/// (session, week, day). The model goes in parentheses; "all models" (or no
/// parentheses) means every model. Only the first window of each kind and
/// model is kept. Other lines (the "What's contributing" section) are ignored.
pub fn parse_usage_text(text: &str) -> Vec<ParsedWindow> {
    let mut windows: Vec<ParsedWindow> = Vec::new();
    for line in text.lines() {
        let Some(window) = parse_line(line.trim()) else {
            continue;
        };
        if !windows
            .iter()
            .any(|w| w.kind == window.kind && w.model == window.model)
        {
            windows.push(window);
        }
    }
    windows
}

fn parse_line(line: &str) -> Option<ParsedWindow> {
    let (start, end, percent) = find_percent(line)?;
    let label = line[..start]
        .trim()
        .trim_end_matches([':', '·', '-', '—'])
        .trim();
    let (name, model) = match (label.find('('), label.rfind(')')) {
        (Some(open), Some(close)) if close > open => {
            let inner = label[open + 1..close].trim();
            let all = inner.to_lowercase();
            let model = (!inner.is_empty() && !all.starts_with("all")).then(|| inner.to_owned());
            (label[..open].trim().to_lowercase(), model)
        }
        _ => (label.to_lowercase(), None),
    };
    let kind = if name.contains("session") || name.contains("5-hour") || name.contains("5 hour") {
        WindowKind::Session
    } else if name.contains("week") || name.contains("7-day") || name.contains("7 day") {
        WindowKind::Week
    } else if name.contains("day") || name.contains("daily") {
        WindowKind::Day
    } else {
        return None;
    };
    let rest = &line[end..];
    let resets_text = rest
        .to_ascii_lowercase()
        .find("reset")
        .map(|i| {
            let after = &rest[i..];
            let skip = after.find(char::is_whitespace).unwrap_or(after.len());
            after[skip..].trim().to_owned()
        })
        .filter(|t| !t.is_empty());
    let reset = resets_text.as_deref().and_then(parse_reset);
    Some(ParsedWindow {
        kind,
        model,
        percent: percent.clamp(0.0, 100.0),
        resets_text,
        reset,
    })
}

/// The first `<number>%` in `line`: (start, end after `%`, value).
fn find_percent(line: &str) -> Option<(usize, usize, f64)> {
    let bytes = line.as_bytes();
    let pct = line.find('%')?;
    let mut start = pct;
    while start > 0
        && (bytes[start - 1].is_ascii_digit() || matches!(bytes[start - 1], b'.' | b','))
    {
        start -= 1;
    }
    let number = line[start..pct].replace(',', ".");
    let value: f64 = number.trim_matches('.').parse().ok()?;
    Some((start, pct + 1, value))
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// "Nov 14, 7pm (America/Caracas)", "Nov 14 at 7:30pm", "7pm", "19:00 (UTC)",
/// "in 3 hr", "in 2h 30m", "in 45 min", "in 1 day".
pub fn parse_reset(text: &str) -> Option<Reset> {
    let text = text.trim();
    let lower = text.to_lowercase();
    if let Some(rel) = lower.strip_prefix("in ") {
        return parse_relative(rel).map(Reset::In);
    }
    let (body, zone) = match (text.find('('), text.rfind(')')) {
        (Some(open), Some(close)) if close > open => {
            let zone = text[open + 1..close].trim();
            let zone = (!zone.is_empty() && !zone.contains(' ')).then(|| zone.to_owned());
            (text[..open].trim().to_lowercase(), zone)
        }
        _ => (lower.clone(), None),
    };
    let words: Vec<&str> = body
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty() && *w != "at" && *w != "on")
        .collect();
    let (month, day, time_words) = match words.as_slice() {
        [m, d, rest @ ..] if month_number(m).is_some() => {
            (month_number(m), Some(d.parse::<u8>().ok()?), rest)
        }
        rest => (None, None, rest),
    };
    if day.is_some_and(|d| !(1..=31).contains(&d)) {
        return None;
    }
    let (hour, minute) = parse_clock(&time_words.join(""))?;
    Some(Reset::At(WallTime {
        month,
        day,
        hour,
        minute,
        zone,
    }))
}

fn month_number(word: &str) -> Option<u8> {
    let word = word.trim_end_matches('.');
    if word.len() < 3 {
        return None;
    }
    MONTHS
        .iter()
        .position(|m| word.starts_with(m))
        .map(|i| i as u8 + 1)
}

/// "7pm", "7:30pm", "12am", "19:00", "7 pm" (spaces already removed).
fn parse_clock(text: &str) -> Option<(u8, u8)> {
    let (digits, meridiem) = if let Some(d) = text.strip_suffix("am") {
        (d, Some(false))
    } else if let Some(d) = text.strip_suffix("pm") {
        (d, Some(true))
    } else {
        (text, None)
    };
    let (h, m) = match digits.split_once(':') {
        Some((h, m)) => (h.parse::<u8>().ok()?, m.parse::<u8>().ok()?),
        None => (digits.parse::<u8>().ok()?, 0),
    };
    if m > 59 {
        return None;
    }
    let hour = match meridiem {
        Some(pm) if (1..=12).contains(&h) => (h % 12) + if pm { 12 } else { 0 },
        Some(_) => return None,
        None if h < 24 => h,
        None => return None,
    };
    Some((hour, m))
}

/// "3 hr", "2h 30m", "45 min", "1 day", "3 hours 5 minutes" → seconds.
fn parse_relative(text: &str) -> Option<i64> {
    let mut total = 0i64;
    let mut number: Option<f64> = None;
    let mut found = false;
    let mut token = String::new();
    let flush =
        |token: &mut String, number: &mut Option<f64>, total: &mut i64, found: &mut bool| {
            if token.is_empty() {
                return true;
            }
            let word = std::mem::take(token);
            if let Ok(n) = word.parse::<f64>() {
                *number = Some(n);
                return true;
            }
            let unit = if word.starts_with('d') {
                86_400.0
            } else if word.starts_with('h') {
                3_600.0
            } else if word.starts_with("mi") || word == "m" {
                60.0
            } else if word.starts_with('s') {
                1.0
            } else {
                return false;
            };
            match number.take() {
                Some(n) => {
                    *total += (n * unit) as i64;
                    *found = true;
                    true
                }
                None => false,
            }
        };
    let mut last_digit: Option<bool> = None;
    for c in text.chars() {
        if c.is_whitespace() || c == ',' {
            if !flush(&mut token, &mut number, &mut total, &mut found) {
                return None;
            }
            last_digit = None;
            continue;
        }
        let digit = c.is_ascii_digit() || c == '.';
        if last_digit.is_some_and(|d| d != digit)
            && !flush(&mut token, &mut number, &mut total, &mut found)
        {
            return None;
        }
        token.push(c);
        last_digit = Some(digit);
    }
    if !flush(&mut token, &mut number, &mut total, &mut found) || number.is_some() {
        return None;
    }
    found.then_some(total)
}

/// Reads the `result` of Codex's `account/rateLimits/read` (app-server,
/// docs/decisions.md §4): `rateLimitsByLimitId` (one snapshot per metered
/// limit) when present, else `rateLimits`. Each snapshot has `primary` and
/// `secondary` windows with `usedPercent` (0–100), `windowDurationMins` and
/// `resetsAt` (Unix seconds). The `codex` limit is shared by every model;
/// other limits are named by `limitName` (or the model slug, or the id).
/// `None` when no window can be read.
pub fn parse_codex_rate_limits(json: &str, checked_at: i64) -> Option<PlanUsage> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let mut snapshots: Vec<(String, &serde_json::Value)> =
        match value["rateLimitsByLimitId"].as_object() {
            Some(map) if !map.is_empty() => map.iter().map(|(id, s)| (id.clone(), s)).collect(),
            _ => vec![(
                value["rateLimits"]["limitId"]
                    .as_str()
                    .unwrap_or("codex")
                    .to_owned(),
                &value["rateLimits"],
            )],
        };
    // The shared limit first.
    snapshots.sort_by_key(|(id, _)| id != "codex");
    let mut windows = Vec::new();
    for (id, snapshot) in snapshots {
        let model = (id != "codex").then(|| {
            ["limitName", "normalModelSlug"]
                .iter()
                .find_map(|k| snapshot[k].as_str().filter(|s| !s.is_empty()))
                .unwrap_or(&id)
                .to_owned()
        });
        for key in ["primary", "secondary"] {
            let w = &snapshot[key];
            let Some(percent) = w["usedPercent"].as_f64() else {
                continue;
            };
            let minutes = w["windowDurationMins"]
                .as_u64()
                .and_then(|m| u32::try_from(m).ok());
            windows.push(UsageWindow {
                kind: minutes.map_or(WindowKind::Other, WindowKind::from_minutes),
                minutes,
                model: model.clone(),
                percent: percent.clamp(0.0, 100.0),
                resets_text: None,
                resets_at: w["resetsAt"].as_i64(),
            });
        }
    }
    (!windows.is_empty()).then_some(PlanUsage {
        windows,
        checked_at,
        source: UsageSource::CodexRateLimits,
        balance: None,
    })
}

/// The currency of Moonshot's balance, by API host: the global platform
/// (`api.moonshot.ai`) reports USD and the China one (`api.moonshot.cn`)
/// CNY. `None` for any other endpoint (Kimi Code, a proxy), which has no
/// documented balance.
pub fn moonshot_currency(base_url: &str) -> Option<&'static str> {
    let host = base_url
        .split("://")
        .nth(1)
        .unwrap_or(base_url)
        .split(['/', ':'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match host.as_str() {
        "api.moonshot.ai" => Some("USD"),
        "api.moonshot.cn" => Some("CNY"),
        _ => None,
    }
}

/// Reads `GET /v1/users/me/balance` (platform.kimi.ai/docs/api/balance):
/// `{"code":0,"data":{"available_balance":49.58,...},"status":true}`.
pub fn parse_moonshot_balance(json: &str, currency: &str, checked_at: i64) -> Option<PlanUsage> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["status"].as_bool() == Some(false) {
        return None;
    }
    let available = value["data"]["available_balance"].as_f64()?;
    Some(PlanUsage {
        windows: Vec::new(),
        checked_at,
        source: UsageSource::MoonshotBalance,
        balance: Some(Balance {
            available,
            currency: currency.to_owned(),
        }),
    })
}

/// Turns parsed windows into a [`PlanUsage`]. `resolve` gives the Unix
/// seconds of a wall time (next occurrence after the check), or `None`.
pub fn resolve_windows(
    windows: Vec<ParsedWindow>,
    checked_at: i64,
    source: UsageSource,
    resolve: impl Fn(&WallTime) -> Option<i64>,
) -> PlanUsage {
    let windows = windows
        .into_iter()
        .map(|w| UsageWindow {
            minutes: match w.kind {
                WindowKind::Session => Some(300),
                WindowKind::Day => Some(1440),
                WindowKind::Week => Some(10080),
                WindowKind::Other => None,
            },
            resets_at: match &w.reset {
                Some(Reset::In(secs)) => Some(checked_at + secs),
                Some(Reset::At(wall)) => resolve(wall),
                None => None,
            },
            kind: w.kind,
            model: w.model,
            percent: w.percent,
            resets_text: w.resets_text,
        })
        .collect();
    PlanUsage {
        windows,
        checked_at,
        source,
        balance: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "You are currently using your subscription to power your Claude Code usage\n\nCurrent session: 34% used · resets Nov 14, 7pm (America/Caracas)\nCurrent week (all models): 29% used · resets Nov 20, 12am (America/Caracas)\nCurrent week (Fable): 0% used · resets Nov 20, 12am (America/Caracas)\n\nWhat's contributing to your limits usage?\nApproximate · sessions on this machine only · shares overlap\n\nLast 24h · 3232 requests · 17 sessions\n  98%  Give \"general-purpose\" subagents a cheaper model\n  83%  Start fresh when you switch tasks · run /clear\n  Top skills: /dataviz 1%\n";

    fn json(result: &str) -> String {
        serde_json::json!({
            "type": "result", "subtype": "success", "is_error": false,
            "num_turns": 0, "total_cost_usd": 0, "local_command": "usage",
            "usage": {"input_tokens": 0, "output_tokens": 0,
                      "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0},
            "result": result,
        })
        .to_string()
    }

    fn caracas(month: u8, day: u8, hour: u8) -> Option<Reset> {
        Some(Reset::At(WallTime {
            month: Some(month),
            day: Some(day),
            hour,
            minute: 0,
            zone: Some("America/Caracas".into()),
        }))
    }

    #[test]
    fn reads_the_real_sample() {
        let ClaudeUsageAnswer::Windows(w) = parse_claude_usage(&json(SAMPLE)) else {
            panic!("expected windows");
        };
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].kind, WindowKind::Session);
        assert_eq!(w[0].model, None);
        assert_eq!(w[0].percent, 34.0);
        assert_eq!(
            w[0].resets_text.as_deref(),
            Some("Nov 14, 7pm (America/Caracas)")
        );
        assert_eq!(w[0].reset, caracas(11, 14, 19));
        assert_eq!(
            (w[1].kind, w[1].model.as_deref(), w[1].percent),
            (WindowKind::Week, None, 29.0)
        );
        assert_eq!(w[1].reset, caracas(11, 20, 0));
        assert_eq!(
            (w[2].kind, w[2].model.as_deref(), w[2].percent),
            (WindowKind::Week, Some("Fable"), 0.0)
        );
    }

    #[test]
    fn tolerates_variants() {
        let text = "Current session: 12.5% used\n\
                    Current week (Opus): 80% used · resets in 3 hr\n\
                    Current week: 41,5% used · Resets Dec 1 at 9:30am\n\
                    Current week (all models): 99% used\n\
                    Current day: 5% used · resets 7pm";
        let w = parse_usage_text(text);
        assert_eq!(w.len(), 4, "{w:?}");
        assert_eq!((w[0].percent, w[0].resets_text.as_deref()), (12.5, None));
        assert_eq!(w[1].model.as_deref(), Some("Opus"));
        assert_eq!(w[1].reset, Some(Reset::In(3 * 3600)));
        // "Current week" without parentheses is the shared week; the later
        // "(all models)" line is a duplicate and is dropped.
        assert_eq!((w[2].model.as_deref(), w[2].percent), (None, 41.5));
        assert_eq!(
            w[2].reset,
            Some(Reset::At(WallTime {
                month: Some(12),
                day: Some(1),
                hour: 9,
                minute: 30,
                zone: None
            }))
        );
        assert_eq!(w[3].kind, WindowKind::Day);
        assert!(matches!(&w[3].reset, Some(Reset::At(t)) if t.hour == 19 && t.month.is_none()));
    }

    #[test]
    fn unknown_resets_keep_their_text() {
        let w = parse_usage_text("Current session: 50% used · resets soon-ish");
        assert_eq!(w[0].resets_text.as_deref(), Some("soon-ish"));
        assert_eq!(w[0].reset, None);
    }

    #[test]
    fn reset_formats() {
        assert_eq!(parse_reset("in 2h 30m"), Some(Reset::In(9000)));
        assert_eq!(parse_reset("in 45 min"), Some(Reset::In(2700)));
        assert_eq!(parse_reset("in 1 day"), Some(Reset::In(86_400)));
        assert_eq!(parse_reset("in 3 hours 5 minutes"), Some(Reset::In(11_100)));
        assert_eq!(parse_reset("in a while"), None);
        assert_eq!(parse_reset("in 3"), None);
        assert_eq!(
            parse_reset("19:00 (UTC)"),
            Some(Reset::At(WallTime {
                month: None,
                day: None,
                hour: 19,
                minute: 0,
                zone: Some("UTC".into())
            }))
        );
        assert!(
            matches!(parse_reset("September 3, 12pm"), Some(Reset::At(t)) if t.month == Some(9) && t.hour == 12)
        );
        assert_eq!(parse_reset("Nov 40, 7pm"), None);
        assert_eq!(parse_reset("13pm"), None);
        assert_eq!(parse_reset("tomorrow"), None);
    }

    #[test]
    fn garbage_is_no_data() {
        for out in [
            "",
            "not json",
            "{}",
            &json(""),
            &json("Unknown command: /usage"),
            &json("Current balance: 12 credits"),
        ] {
            assert_eq!(parse_claude_usage(out), ClaudeUsageAnswer::NoData, "{out}");
        }
        let mut error: serde_json::Value = serde_json::from_str(&json(SAMPLE)).unwrap();
        error["is_error"] = true.into();
        assert_eq!(
            parse_claude_usage(&error.to_string()),
            ClaudeUsageAnswer::NoData
        );
    }

    #[test]
    fn a_model_call_trips_the_guard() {
        for (key, value) in [
            ("num_turns", serde_json::json!(1)),
            ("total_cost_usd", serde_json::json!(0.002)),
        ] {
            let mut out: serde_json::Value = serde_json::from_str(&json(SAMPLE)).unwrap();
            out[key] = value;
            assert_eq!(
                parse_claude_usage(&out.to_string()),
                ClaudeUsageAnswer::SpentTokens
            );
        }
        let mut out: serde_json::Value = serde_json::from_str(&json(SAMPLE)).unwrap();
        out["usage"]["output_tokens"] = 12.into();
        assert_eq!(
            parse_claude_usage(&out.to_string()),
            ClaudeUsageAnswer::SpentTokens
        );
    }

    #[test]
    fn resolving_and_headline() {
        let ClaudeUsageAnswer::Windows(w) = parse_claude_usage(&json(SAMPLE)) else {
            panic!()
        };
        let usage = resolve_windows(w, 1000, UsageSource::ClaudeUsage, |t| {
            Some(i64::from(t.hour) * 10)
        });
        assert_eq!(usage.session().unwrap().resets_at, Some(190));
        assert_eq!(usage.week().unwrap().minutes, Some(10080));
        assert_eq!(usage.headline().unwrap().percent, 34.0);
        assert_eq!(usage.model_windows().count(), 1);

        let relative = resolve_windows(
            parse_usage_text("Current session: 1% used · resets in 2 hr"),
            1000,
            UsageSource::ClaudeUsage,
            |_| None,
        );
        assert_eq!(relative.windows[0].resets_at, Some(1000 + 7200));
    }

    #[test]
    fn serializes_for_the_ui() {
        let usage = resolve_windows(
            parse_usage_text("Current session: 1% used"),
            5,
            UsageSource::ClaudeUsage,
            |_| None,
        );
        let json = serde_json::to_value(&usage).unwrap();
        assert_eq!(json["source"], "claudeUsage");
        assert_eq!(json["checkedAt"], 5);
        assert_eq!(json["windows"][0]["kind"], "session");
        let back: PlanUsage = serde_json::from_value(json).unwrap();
        assert_eq!(back, usage);
    }

    #[test]
    fn reads_the_moonshot_balance() {
        let body = r#"{"code":0,"data":{"available_balance":49.58894,"voucher_balance":46.58893,"cash_balance":3.00001},"scode":"0x0","status":true}"#;
        let usage = parse_moonshot_balance(body, "USD", 3).unwrap();
        assert_eq!(usage.source, UsageSource::MoonshotBalance);
        assert_eq!(
            usage.balance,
            Some(Balance {
                available: 49.58894,
                currency: "USD".into()
            })
        );
        assert!(usage.headline().is_none());
        assert_eq!(parse_moonshot_balance("{}", "USD", 3), None);
        assert_eq!(
            parse_moonshot_balance(
                r#"{"status":false,"data":{"available_balance":1}}"#,
                "USD",
                3
            ),
            None
        );
        assert_eq!(moonshot_currency("https://api.moonshot.ai/v1"), Some("USD"));
        assert_eq!(
            moonshot_currency("https://api.moonshot.cn/v1/"),
            Some("CNY")
        );
        assert_eq!(moonshot_currency("https://api.kimi.com/coding/v1"), None);
        assert_eq!(moonshot_currency("http://127.0.0.1:9/v1"), None);
    }

    #[test]
    fn reads_codex_rate_limits() {
        // Shape of the official docs example (learn.chatgpt.com/docs/app-server).
        let json = serde_json::json!({
            "rateLimits": {"limitId": "codex", "limitName": null,
                "primary": {"usedPercent": 25, "windowDurationMins": 300, "resetsAt": 1730947200},
                "secondary": {"usedPercent": 60, "windowDurationMins": 10080, "resetsAt": 1731500000}},
            "rateLimitsByLimitId": {
                "codex": {"limitId": "codex",
                    "primary": {"usedPercent": 25, "windowDurationMins": 300, "resetsAt": 1730947200},
                    "secondary": {"usedPercent": 60, "windowDurationMins": 10080, "resetsAt": 1731500000}},
                "codex_other": {"limitName": "codex_other",
                    "primary": {"usedPercent": 42, "windowDurationMins": 60, "resetsAt": 1730950800},
                    "secondary": null}
            }
        })
        .to_string();
        let usage = parse_codex_rate_limits(&json, 7).unwrap();
        assert_eq!(usage.source, UsageSource::CodexRateLimits);
        assert_eq!(usage.session().unwrap().percent, 25.0);
        assert_eq!(usage.week().unwrap().resets_at, Some(1731500000));
        let other: Vec<_> = usage.model_windows().collect();
        assert_eq!(other.len(), 1);
        assert_eq!(other[0].model.as_deref(), Some("codex_other"));
        assert_eq!(
            (other[0].kind, other[0].minutes),
            (WindowKind::Other, Some(60))
        );
        assert_eq!(usage.headline().unwrap().percent, 60.0);

        // Older servers: only the single-bucket view, secondary may be null.
        let old = r#"{"rateLimits":{"primary":{"usedPercent":5,"windowDurationMins":300,"resetsAt":1},"secondary":null}}"#;
        let usage = parse_codex_rate_limits(old, 7).unwrap();
        assert_eq!(usage.windows.len(), 1);

        for garbage in ["", "{}", r#"{"rateLimits":{"primary":null}}"#] {
            assert_eq!(parse_codex_rate_limits(garbage, 7), None, "{garbage}");
        }
    }
}
