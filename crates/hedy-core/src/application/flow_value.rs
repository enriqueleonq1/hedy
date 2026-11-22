//! How a project's flows did against its tasks with one agent, from the
//! user's own history (docs/decisions.md §52). Hedy's own measurements hold
//! for its eval cases only, so these are the only numbers it shows about what
//! a flow is worth.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::Serialize;

use super::{AppResult, Hedy};
use crate::domain::{Task, TaskMode, TaskStatus};

/// The fewest closed tasks, with the flow and with one agent, before
/// comparing: fewer say more about the tasks than about the flow.
pub const FLOW_VALUE_MIN: usize = 10;

/// A flow's closed tasks in a project, beside the project's tasks with one
/// agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowValue {
    /// The flow's name, as its runs carried it.
    pub flow: String,
    pub runs: u32,
    /// Of them, the ones whose changes the user accepted.
    pub accepted: u32,
    /// Median tokens per task.
    pub tokens: u64,
    /// The same, for the project's tasks with one agent.
    pub alone_runs: u32,
    pub alone_accepted: u32,
    pub alone_tokens: u64,
}

#[derive(Default)]
struct Tally {
    tokens: Vec<u64>,
    accepted: u32,
}

impl Tally {
    fn add(&mut self, tokens: u64, accepted: bool) {
        self.tokens.push(tokens);
        self.accepted += u32::from(accepted);
    }

    fn median(&self) -> u64 {
        let mut sorted = self.tokens.clone();
        sorted.sort_unstable();
        sorted.get(sorted.len() / 2).copied().unwrap_or(0)
    }
}

impl Hedy {
    /// Each flow with enough closed tasks in `project`, against its tasks
    /// with one agent; empty until both sides have [`FLOW_VALUE_MIN`].
    pub fn flow_values(&self, project: &Path) -> AppResult<Vec<FlowValue>> {
        let spent: HashMap<_, _> = self.store.usage_by_task(project)?.into_iter().collect();
        let tokens = |t: &Task| {
            spent
                .get(&t.id)
                .map(|u| u.total_tokens())
                .filter(|n| *n > 0)
        };
        let mut alone = Tally::default();
        let mut flows: BTreeMap<String, Tally> = BTreeMap::new();
        for task in self.store.tasks(project)? {
            if !task.status.is_closed() || task.mode != TaskMode::Project || task.conversation {
                continue;
            }
            let Some(n) = tokens(&task) else { continue };
            let accepted = task.status == TaskStatus::Accepted;
            match &task.flow {
                None => alone.add(n, accepted),
                Some(run) => flows
                    .entry(run.flow.name.clone())
                    .or_default()
                    .add(n, accepted),
            }
        }
        if alone.tokens.len() < FLOW_VALUE_MIN {
            return Ok(Vec::new());
        }
        Ok(flows
            .into_iter()
            .filter(|(_, t)| t.tokens.len() >= FLOW_VALUE_MIN)
            .map(|(flow, t)| FlowValue {
                flow,
                runs: t.tokens.len() as u32,
                accepted: t.accepted,
                tokens: t.median(),
                alone_runs: alone.tokens.len() as u32,
                alone_accepted: alone.accepted,
                alone_tokens: alone.median(),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::NewTask;
    use crate::application::ports::{TaskRepository, UsageRecord, UsageRepository};
    use crate::application::testing::Fixture;
    use crate::domain::UsageSample;
    use crate::domain::flow::{FlowRun, Template, builtin_role};

    fn closed(f: &Fixture, flow: Option<&str>, accepted: bool, tokens: u64) {
        let started = f
            .hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/tienda"),
                prompt: "Cobros".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: false,
                permissions: None,
            })
            .unwrap();
        let mut task = f.hedy.task(&started.id).unwrap();
        task.status = if accepted {
            TaskStatus::Accepted
        } else {
            TaskStatus::Discarded
        };
        task.flow = flow.map(|name| {
            let mut flow = Template::Essential.flow(&Default::default());
            flow.name = name.into();
            FlowRun::new(flow, &builtin_role).unwrap()
        });
        f.store.save_task(&task).unwrap();
        f.store
            .record_usage(&UsageRecord {
                at: 1,
                account: task.account.clone(),
                project: task.project.clone(),
                task: task.id.clone(),
                sample: UsageSample {
                    input_tokens: tokens,
                    ..Default::default()
                },
            })
            .unwrap();
    }

    #[test]
    fn a_flow_is_compared_only_with_enough_closed_tasks_on_both_sides() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let dir = f.dir("/code/tienda");
        f.hedy.use_account(&dir, "personal", None).unwrap();
        for i in 0..10 {
            closed(&f, None, i < 8, 100_000);
        }
        for i in 0..9 {
            closed(&f, Some("Revisión triple"), i < 8, 400_000);
        }
        // Nine runs of the flow: not yet.
        assert_eq!(f.hedy.flow_values(&dir).unwrap(), vec![]);
        closed(&f, Some("Revisión triple"), true, 420_000);
        assert_eq!(
            f.hedy.flow_values(&dir).unwrap(),
            vec![FlowValue {
                flow: "Revisión triple".into(),
                runs: 10,
                accepted: 9,
                tokens: 400_000,
                alone_runs: 10,
                alone_accepted: 8,
                alone_tokens: 100_000,
            }]
        );
    }
}
