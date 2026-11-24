//! `--checkpoints`: does stopping at a plan for the user's word help, and
//! does it matter whether the session that planned goes on to implement?
//! (evals/experiments/checkpoints). The request leaves out something only the
//! user knows (`checkpoint.clarify` in `case.json`):
//!
//! - `solo`: one AI with the request, as Hedy works by default;
//! - `informed`: one AI with the request and the clarification together;
//! - `plan`: one AI with «Enséñame el plan primero»: the user answers the
//!   first plan with the clarification and approves the next, and the same
//!   session implements;
//! - `approval`: the «Con tu aprobación» flow: an architect plans in its own
//!   session, the user answers the first approval with the clarification and
//!   approves the next, and a programmer implements in a new session, with
//!   the tests as the gate.

use hedy_core::domain::flow::Template;

use crate::CheckpointSpec;

pub const ARMS: [&str; 4] = ["solo", "informed", "plan", "approval"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arm {
    Solo,
    Informed,
    Plan,
    Approval,
}

impl Arm {
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "solo" => Some(Self::Solo),
            "informed" => Some(Self::Informed),
            "plan" => Some(Self::Plan),
            "approval" => Some(Self::Approval),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Solo => "solo",
            Self::Informed => "informed",
            Self::Plan => "plan",
            Self::Approval => "approval",
        }
    }

    /// What the user asks: the request, with the clarification only when
    /// `informed`.
    pub fn prompt(self, spec: &CheckpointSpec, case_prompt: &str) -> String {
        let request = spec.prompt.as_deref().unwrap_or(case_prompt);
        match self {
            Self::Informed => format!("{request}\n\n{}", spec.clarify),
            _ => request.to_owned(),
        }
    }

    /// The flow the task starts with (`none`: one AI).
    pub fn flow(self) -> &'static str {
        match self {
            Self::Approval => Template::Approval.key(),
            _ => "none",
        }
    }

    pub fn plan_first(self) -> bool {
        self == Self::Plan
    }

    /// The user answers its first checkpoint with the clarification.
    pub fn corrects(self) -> bool {
        matches!(self, Self::Plan | Self::Approval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(prompt: Option<&str>) -> CheckpointSpec {
        CheckpointSpec {
            prompt: prompt.map(str::to_owned),
            clarify: "Ojo: siempre dos decimales.".into(),
        }
    }

    #[test]
    fn only_the_informed_arm_hears_the_clarification_up_front() {
        for key in ARMS {
            let arm = Arm::from_key(key).unwrap();
            assert_eq!(arm.key(), key);
            let prompt = arm.prompt(&spec(None), "Formatea el precio.");
            assert_eq!(prompt.contains("dos decimales"), arm == Arm::Informed);
            assert!(prompt.starts_with("Formatea el precio."));
        }
        assert_eq!(Arm::from_key("flow"), None);
    }

    #[test]
    fn the_test_can_ask_its_own_request() {
        let prompt = Arm::Solo.prompt(&spec(Some("Pedido corto.")), "Pedido largo.");
        assert_eq!(prompt, "Pedido corto.");
    }

    #[test]
    fn the_checkpoint_arms_stop_at_a_plan() {
        assert!(Arm::Plan.plan_first() && Arm::Plan.corrects());
        assert_eq!(Arm::Plan.flow(), "none");
        assert!(!Arm::Approval.plan_first() && Arm::Approval.corrects());
        assert_eq!(Arm::Approval.flow(), "approval");
        assert!(!Arm::Solo.corrects() && !Arm::Informed.corrects());
    }
}
