//! The common event language every agent adapter translates into (spec §4).
//! Serializable so the history of a task can be stored and replayed.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AgentEvent {
    /// What the user asked (first prompt or a follow-up), with the images
    /// attached to it.
    UserMessage {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<super::Attachment>,
    },
    SessionStarted {
        session_id: String,
        model: String,
    },
    /// Text from the agent.
    Message {
        text: String,
    },
    /// A plan proposed for approval. `request_id` is set while it awaits an answer.
    Plan {
        text: String,
        request_id: Option<String>,
    },
    /// The agent's checklist (TodoWrite), replaced as a whole on every update.
    Steps {
        steps: Vec<PlanStep>,
    },
    /// The agent wants to run a tool that needs approval ("PideComando").
    PermissionRequest {
        request_id: String,
        tool: String,
        summary: String,
        input: serde_json::Value,
    },
    PermissionResolved {
        request_id: String,
        decision: Decision,
        /// Answered by a rule instead of the user.
        automatic: bool,
        reason: Option<String>,
    },
    FileEdited {
        path: String,
    },
    CommandRun {
        command: String,
        tests: bool,
    },
    /// Result of a test command ("CorrioTests").
    TestsRun {
        command: String,
        passed: bool,
    },
    /// A tool starts running: its name as the agent calls it, and the command
    /// or the file it works on. Live only (the mascot's activity): never
    /// stored, the history keeps the finished tool.
    ToolStarted {
        tool: String,
        detail: String,
    },
    /// Any other tool (read, search, web...), for the activity lines.
    ToolUsed {
        tool: String,
        summary: String,
    },
    /// A phase (or a handoff) starts with an agent: "Codex implementa".
    PhaseStarted {
        phase: super::Phase,
        /// "Claude Opus", "Codex", "Kimi".
        agent: String,
        account: String,
    },
    Usage(UsageSample),
    /// The account is close to (or at) its limit. Hedy never switches
    /// accounts by itself: the user chooses to wait or to hand the task over.
    LimitWarning {
        account: String,
        level: LimitLevel,
        percent: Option<f64>,
        resets_at: Option<i64>,
    },
    RateLimit {
        status: String,
        /// Percentage used (0–100), when reported.
        utilization: Option<f64>,
        resets_at: Option<i64>,
    },
    /// A flow reached a step where the user approves what the roles before
    /// it did (docs/decisions.md §42).
    FlowApproval {
        node: String,
    },
    /// The agent finished its turn ("Termino").
    Finished {
        ok: bool,
        summary: String,
        duration_ms: Option<u64>,
    },
    Error {
        message: String,
    },
    /// The slash commands the agent's CLI offers in this session, as it
    /// reports them. Kept per account, never in a task's history.
    Commands {
        commands: Vec<super::AgentCommand>,
    },
}

impl AgentEvent {
    /// A user message without images.
    pub fn user(text: impl Into<String>) -> Self {
        Self::UserMessage {
            text: text.into(),
            images: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LimitLevel {
    Warning,
    Reached,
}

/// Percentage from which Hedy warns about a limit.
const LIMIT_WARNING_PERCENT: f64 = 90.0;

impl LimitLevel {
    /// From a CLI's report: Claude's `status` ("allowed", "allowed_warning",
    /// "rejected") and/or a 0–100 percentage (Codex).
    pub fn from_report(status: &str, percent: Option<f64>) -> Option<Self> {
        if status == "rejected" || percent.is_some_and(|p| p >= 100.0) {
            Some(Self::Reached)
        } else if status == "allowed_warning" || percent.is_some_and(|p| p >= LIMIT_WARNING_PERCENT)
        {
            Some(Self::Warning)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub text: String,
    pub status: StepStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepStatus {
    Pending,
    InProgress,
    Done,
}

/// The three answers of a permission card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Allow,
    /// "Permitir en este chat": allow and remember for the rest of the task.
    AllowForTask,
    Deny,
}

impl Decision {
    pub fn allows(self) -> bool {
        !matches!(self, Self::Deny)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSample {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// The CLI's own estimate; for subscriptions it is not what is billed.
    pub cost_usd: Option<f64>,
}

impl UsageSample {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_read_tokens + self.cache_write_tokens
    }

    /// Adds `other` to this total. The cost stays unknown only if both are.
    pub fn add(&mut self, other: &UsageSample) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_write_tokens += other.cache_write_tokens;
        self.cost_usd = match (self.cost_usd, other.cost_usd) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
        };
    }
}

/// Whether a shell command looks like a test run.
pub fn looks_like_tests(command: &str) -> bool {
    const RUNNERS: &[&str] = &[
        "cargo test",
        "cargo nextest",
        "pytest",
        "python -m pytest",
        "npm test",
        "npm run test",
        "pnpm test",
        "yarn test",
        "bun test",
        "vitest",
        "jest",
        "go test",
        "mix test",
        "rspec",
        "phpunit",
    ];
    RUNNERS.iter().any(|r| command.contains(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_serialize_with_a_kind_tag() {
        let event = AgentEvent::Plan {
            text: "1. Hacer".into(),
            request_id: Some("r1".into()),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "plan");
        assert_eq!(json["requestId"], "r1");
        let back: AgentEvent = serde_json::from_value(json).unwrap();
        assert_eq!(back, event);

        let usage = AgentEvent::Usage(UsageSample {
            input_tokens: 3,
            ..Default::default()
        });
        assert_eq!(serde_json::to_value(&usage).unwrap()["inputTokens"], 3);
    }

    #[test]
    fn limit_levels() {
        assert_eq!(LimitLevel::from_report("allowed", None), None);
        assert_eq!(
            LimitLevel::from_report("allowed_warning", None),
            Some(LimitLevel::Warning)
        );
        assert_eq!(
            LimitLevel::from_report("rejected", None),
            Some(LimitLevel::Reached)
        );
        assert_eq!(
            LimitLevel::from_report("ok", Some(92.0)),
            Some(LimitLevel::Warning)
        );
        assert_eq!(
            LimitLevel::from_report("ok", Some(100.0)),
            Some(LimitLevel::Reached)
        );
        assert_eq!(LimitLevel::from_report("ok", Some(40.0)), None);
    }

    #[test]
    fn detects_test_commands() {
        assert!(looks_like_tests("cd api && cargo test -p core"));
        assert!(looks_like_tests("pytest tests/test_x.py"));
        assert!(!looks_like_tests("ls -la"));
    }
}
