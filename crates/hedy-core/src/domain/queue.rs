//! Messages the user writes while an agent is working ("En cola"). They wait
//! for the end of the agent's turn and go, all together, as the next user turn
//! of the same session: never in the middle of a turn, never while a plan or a
//! permission card waits for the user, and never to another account.

use serde::{Deserialize, Serialize};

use super::TaskId;

/// Why queued messages are on hold. Held messages are only sent when the user
/// says so: after a limit Hedy sends nothing by itself (rule 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HoldReason {
    /// The task failed.
    Failed,
    /// The user stopped the agent.
    Stopped,
    /// The account reached its limit.
    Limit,
}

impl HoldReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Limit => "limit",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "failed" => Some(Self::Failed),
            "stopped" => Some(Self::Stopped),
            "limit" => Some(Self::Limit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedMessage {
    pub id: i64,
    pub task: TaskId,
    pub text: String,
    /// Seconds.
    pub created_at: i64,
    /// Set when the task stopped before the message could go.
    pub held: Option<HoldReason>,
}

/// Several queued messages travel as one user turn, in the order written,
/// separated by a blank line: one turn is the same for every provider and
/// keeps the agent from answering half of what the user said.
pub fn join_queued(messages: &[QueuedMessage]) -> String {
    messages
        .iter()
        .map(|m| m.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: i64, text: &str) -> QueuedMessage {
        QueuedMessage {
            id,
            task: TaskId::new("t1"),
            text: text.into(),
            created_at: 0,
            held: None,
        }
    }

    #[test]
    fn queued_messages_join_in_order() {
        let joined = join_queued(&[
            message(1, " y los tests "),
            message(2, ""),
            message(3, "usa pytest"),
        ]);
        assert_eq!(joined, "y los tests\n\nusa pytest");
    }

    #[test]
    fn hold_reasons_round_trip() {
        for reason in [HoldReason::Failed, HoldReason::Stopped, HoldReason::Limit] {
            assert_eq!(HoldReason::parse(reason.as_str()), Some(reason));
        }
        assert_eq!(HoldReason::parse("x"), None);
    }
}
