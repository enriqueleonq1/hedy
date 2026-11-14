//! Phases of a task and who works on each one (spec: "modelo por fase").

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::{AccountId, DomainError, TaskMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Plan,
    Implement,
    Tests,
    Review,
}

impl Phase {
    pub const ALL: [Phase; 4] = [Self::Plan, Self::Implement, Self::Tests, Self::Review];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Implement => "implement",
            Self::Tests => "tests",
            Self::Review => "review",
        }
    }

    /// Whether an agent on this phase in `mode` only reads (gets no write
    /// tools) for its whole session. Planning is not: the same session goes on
    /// implementing once the plan is approved, and `plan_first` holds writes
    /// until then.
    pub fn read_only_in(self, mode: TaskMode) -> bool {
        mode != TaskMode::Project || self == Self::Review
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Phase {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|p| p.as_str() == s)
            .ok_or_else(|| DomainError::UnknownPhase(s.to_owned()))
    }
}

/// Who works on a phase. `model: None` lets the provider choose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub account: AccountId,
    pub model: Option<String>,
}

/// Per-project configuration. Missing phases: `Plan`/`Implement` use the
/// project's account; `Tests`/`Review` are skipped.
pub type PhasePlan = BTreeMap<Phase, Assignment>;

/// One step of a task's pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    pub phase: Phase,
    pub account: AccountId,
    pub model: Option<String>,
}

impl Stage {
    /// Same account and model: the stages can share one agent session.
    pub fn same_agent(&self, other: &Stage) -> bool {
        self.account == other.account && self.model == other.model
    }
}

/// The stages a new task runs through.
///
/// - `model_override` (a model picked in the composer) runs everything with the
///   project's account and that model, without the optional phases.
/// - Questions and research are a single read-only stage.
pub fn pipeline(
    project_account: &AccountId,
    phases: &PhasePlan,
    mode: TaskMode,
    plan_first: bool,
    model_override: Option<&str>,
) -> Vec<Stage> {
    let stage = |phase: Phase| -> Stage {
        match (model_override, phases.get(&phase)) {
            (Some(model), _) => Stage {
                phase,
                account: project_account.clone(),
                model: Some(model.to_owned()),
            },
            (None, Some(a)) => Stage {
                phase,
                account: a.account.clone(),
                model: a.model.clone(),
            },
            (None, None) => Stage {
                phase,
                account: project_account.clone(),
                model: None,
            },
        }
    };
    if !mode.changes_files() {
        return vec![stage(Phase::Implement)];
    }
    let mut stages = Vec::new();
    if plan_first {
        stages.push(stage(Phase::Plan));
    }
    stages.push(stage(Phase::Implement));
    if model_override.is_none() {
        for optional in [Phase::Tests, Phase::Review] {
            if phases.contains_key(&optional) {
                stages.push(stage(optional));
            }
        }
    }
    stages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> AccountId {
        AccountId::new(s).unwrap()
    }

    fn assign(account: &str, model: Option<&str>) -> Assignment {
        Assignment {
            account: id(account),
            model: model.map(str::to_owned),
        }
    }

    #[test]
    fn default_pipeline_uses_the_project_account() {
        let stages = pipeline(
            &id("claude"),
            &PhasePlan::new(),
            TaskMode::Project,
            true,
            None,
        );
        let phases: Vec<_> = stages.iter().map(|s| s.phase).collect();
        assert_eq!(phases, vec![Phase::Plan, Phase::Implement]);
        assert!(stages[0].same_agent(&stages[1]));
    }

    #[test]
    fn configured_phases_split_the_work() {
        let plan = PhasePlan::from([
            (Phase::Plan, assign("claude", Some("opus"))),
            (Phase::Implement, assign("codex", None)),
            (Phase::Tests, assign("kimi", Some("kimi-k3.1-pro"))),
        ]);
        let stages = pipeline(&id("claude"), &plan, TaskMode::Project, true, None);
        let who: Vec<_> = stages
            .iter()
            .map(|s| (s.phase, s.account.as_str()))
            .collect();
        assert_eq!(
            who,
            vec![
                (Phase::Plan, "claude"),
                (Phase::Implement, "codex"),
                (Phase::Tests, "kimi")
            ]
        );
        assert!(!stages[0].same_agent(&stages[1]));

        let no_plan = pipeline(&id("claude"), &plan, TaskMode::Project, false, None);
        assert_eq!(no_plan[0].phase, Phase::Implement);
    }

    #[test]
    fn a_model_override_keeps_it_simple() {
        let plan = PhasePlan::from([(Phase::Tests, assign("kimi", None))]);
        let stages = pipeline(&id("claude"), &plan, TaskMode::Project, true, Some("haiku"));
        assert_eq!(stages.len(), 2);
        assert!(
            stages
                .iter()
                .all(|s| s.account.as_str() == "claude" && s.model.as_deref() == Some("haiku"))
        );
    }

    #[test]
    fn questions_are_one_stage() {
        let plan = PhasePlan::from([(Phase::Implement, assign("codex", None))]);
        let stages = pipeline(&id("claude"), &plan, TaskMode::Ask, true, None);
        assert_eq!(
            stages,
            vec![Stage {
                phase: Phase::Implement,
                account: id("codex"),
                model: None
            }]
        );
    }

    #[test]
    fn phases_round_trip() {
        for p in Phase::ALL {
            assert_eq!(p.as_str().parse::<Phase>().unwrap(), p);
        }
        assert!(Phase::Review.read_only_in(TaskMode::Project));
        assert!(
            !Phase::Plan.read_only_in(TaskMode::Project),
            "the planner implements next"
        );
        assert!(Phase::Implement.read_only_in(TaskMode::Ask));
    }
}
