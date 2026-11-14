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
    /// What the user asked (first prompt or a follow-up).
    UserMessage {
        text: String,
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
    /// Any other tool (read, search, web...), for the activity lines.
    ToolUsed {
        tool: String,
        summary: String,
    },
    Usage(UsageSample),
    RateLimit {
        status: String,
        utilization: Option<f64>,
        resets_at: Option<i64>,
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
    fn detects_test_commands() {
        assert!(looks_like_tests("cd api && cargo test -p core"));
        assert!(looks_like_tests("pytest tests/test_x.py"));
        assert!(!looks_like_tests("ls -la"));
    }
}
