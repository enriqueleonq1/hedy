//! Running a flow in a task (docs/decisions.md §42). Each role's step is one
//! more stage of the task, so it gets everything a phase already has: its own
//! agent session, its account's usage and limits, handoffs, permission cards
//! and modes. The flow decides which step comes next.

use super::tasks::NewTask;
use super::{AppError, AppResult, Hedy};
use crate::domain::flow::{Delivery, FlowRun, Next, NodeKind, Outcome, Seat};
use crate::domain::task::title_from_prompt;
use crate::domain::{
    AgentEvent, NoticeKind, Stage, Task, TaskId, TaskMode, TaskStatus, policy, prompts,
};
use crate::tr;

impl Hedy {
    /// Starts a task that runs `which` (a template key or a flow's name) of
    /// the project. Only a task that changes code: the roles share its copy.
    pub fn start_flow(&self, request: NewTask, which: &str) -> AppResult<Task> {
        let project = self.project_at(&request.project)?;
        if request.mode != TaskMode::Project {
            return Err(AppError::InvalidFlow(vec![tr!(
                "un flujo trabaja en una copia de tu proyecto: elige «Cambiar código»",
                "a flow works in a copy of your project: choose «Change code»"
            )]));
        }
        if request.prompt.trim().is_empty() {
            return Err(AppError::EmptyPrompt);
        }
        let main = project.account.clone().ok_or(AppError::NoAccountResolved)?;
        let entry = self.flow(Some(&project.path), which)?;
        let lookup = self.role_lookup(Some(&project.path))?;
        let problems: Vec<String> = entry
            .flow
            .problems(&|id| lookup(id))
            .iter()
            .map(|p| p.message())
            .collect();
        if !problems.is_empty() {
            return Err(AppError::InvalidFlow(problems));
        }
        let run = FlowRun::new(entry.flow, &|id| lookup(id))
            .ok_or_else(|| AppError::InvalidFlow(Vec::new()))?;
        if run
            .flow
            .nodes
            .iter()
            .any(|n| matches!(n.kind, NodeKind::Condition(_) | NodeKind::Parallel { .. }))
        {
            return Err(AppError::FlowNotYet);
        }
        // Every role's account may work here and is ready, before anything is created.
        let projects = self.projects()?;
        for (index, seat) in run.flow.seats().enumerate() {
            let id = seat.account.clone().unwrap_or_else(|| main.clone());
            policy::check_launch(Some(&project), &id, &projects)?;
            let account = self.account(id.as_str())?;
            self.backend(&account)?;
            if index == 0 {
                self.check_images(&request.images, &account)?;
            }
        }

        let now = self.clock.now_ms();
        let id = self.new_task_id(now);
        let title = title_from_prompt(&request.prompt);
        let workspace = Some(self.create_workspace(&project.path, &title, &id)?);
        let code_map = Some(!self.code_map_servers(&project.path)?.is_empty());
        let mut task = Task {
            id,
            project: project.path.clone(),
            mode: request.mode,
            title,
            prompt: request.prompt.clone(),
            account: main,
            model: None,
            workspace,
            scratch: None,
            permissions: request.permissions.unwrap_or(project.permission_mode),
            flow: Some(run),
            session_id: None,
            status: TaskStatus::Running,
            created_at: now / 1000,
            stages: Vec::new(),
            stage: 0,
            plan: None,
            merge: None,
            wait: None,
            conversation: false,
            ship: None,
            origin: None,
            code_map,
            images: Vec::new(),
        };
        task.images = self.store_images(&task.id, &request.images)?;
        self.store.save_task(&task)?;
        self.record(
            &task.id,
            AgentEvent::UserMessage {
                text: request.prompt,
                images: task.images.clone(),
            },
        )?;
        self.flow_step(&mut task)?;
        self.events.task_updated(&task);
        Ok(task)
    }

    /// Runs the flow from where it is until a step that takes time: a role
    /// (its agent starts), an approval (it waits for the user) or the end.
    fn flow_step(&self, task: &mut Task) -> AppResult<()> {
        loop {
            let Some(node) = task.flow.as_ref().and_then(|r| r.current()).cloned() else {
                return self.end_flow(task, None);
            };
            match node.kind {
                NodeKind::Start => {
                    if !self.advance_flow(task)? {
                        return Ok(());
                    }
                }
                NodeKind::Role(seat) => return self.start_role(task, &seat),
                NodeKind::Approval => {
                    self.store.save_task(task)?;
                    self.record(&task.id, AgentEvent::FlowApproval { node: node.id })?;
                    let waiting = task.status == TaskStatus::WaitingApproval;
                    self.set_status(task, TaskStatus::WaitingApproval)?;
                    if !waiting {
                        self.notify(task, NoticeKind::PlanReady);
                    }
                    return Ok(());
                }
                NodeKind::End => return self.end_flow(task, None),
                NodeKind::Condition(_) | NodeKind::Parallel { .. } => {
                    return Err(AppError::FlowNotYet);
                }
            }
        }
    }

    /// Moves past the current step. `false` when the run stopped there (no
    /// way on, or a loop out of rounds).
    fn advance_flow(&self, task: &mut Task) -> AppResult<bool> {
        let Some(run) = task.flow.as_mut() else {
            return Ok(false);
        };
        match run.position.advance(&run.flow, Outcome::Done) {
            Some(Next::Go(_)) => {
                self.store.save_task(task)?;
                Ok(true)
            }
            Some(Next::OutOfRounds { max, .. }) => {
                let note = tr!(
                    "El flujo ya dio las {max} vueltas permitidas: revisa lo que hay y decide.",
                    "The flow already went round the {max} times allowed: look at what there is and decide."
                );
                self.end_flow(task, Some(note))?;
                Ok(false)
            }
            None => {
                self.end_flow(task, None)?;
                Ok(false)
            }
        }
    }

    /// A role's agent starts on its own stage, with what the roles before it
    /// handed over.
    fn start_role(&self, task: &mut Task, seat: &Seat) -> AppResult<()> {
        let run = task.flow.as_ref().expect("a flow task");
        let role = run
            .roles
            .get(&seat.role)
            .cloned()
            .ok_or_else(|| AppError::InvalidFlow(vec![seat.role.clone()]))?;
        let account = match &seat.account {
            Some(account) => account.clone(),
            None => self
                .project_at(&task.project)?
                .account
                .ok_or(AppError::NoAccountResolved)?,
        };
        let base = task
            .workspace
            .as_ref()
            .map(|w| w.base_commit.clone())
            .unwrap_or_default();
        let prompt = prompts::role_prompt(&role, &task.prompt, &run.delivered, &base);
        task.stages.push(Stage {
            phase: role.phase(),
            account,
            model: seat.model.clone(),
        });
        let index = task.stages.len() - 1;
        // Each role is a session of its own: the CLIs cannot share one.
        self.stop_session(&task.id);
        self.set_status(task, TaskStatus::Running)?;
        self.run_stage(task, index, prompt, None)
    }

    /// The end of a run: the changes wait for the user's review, as any task's.
    fn end_flow(&self, task: &mut Task, note: Option<String>) -> AppResult<()> {
        self.stop_session(&task.id);
        if let Some(message) = note {
            self.record(&task.id, AgentEvent::Error { message })?;
        }
        self.store.save_task(task)?;
        let changed = task.status != TaskStatus::Review;
        self.set_status(task, TaskStatus::Review)?;
        if changed {
            self.notify(task, NoticeKind::Finished { changes: true });
        }
        Ok(())
    }

    /// A role's turn ended well: what it said goes to the roles after it, and
    /// the flow moves on.
    pub(super) fn flow_role_done(&self, task: &mut Task, summary: &str) -> AppResult<()> {
        let Some(run) = task.flow.as_mut() else {
            return Ok(());
        };
        if let Some(NodeKind::Role(seat)) = run.current().map(|n| n.kind.clone()) {
            let role = run
                .roles
                .get(&seat.role)
                .map_or_else(|| seat.role.clone(), |r| r.name.clone());
            let text = if summary.trim().is_empty() {
                tr!("(terminó sin resumen)", "(finished without a summary)")
            } else {
                summary.trim().to_owned()
            };
            run.delivered.push(Delivery {
                node: run.position.node.clone(),
                role,
                text,
            });
        }
        if self.advance_flow(task)? {
            self.flow_step(task)?;
        }
        Ok(())
    }

    /// The user's answer at an approval step: on, or back to the role before
    /// it with what to change.
    pub fn answer_flow_approval(
        &self,
        id: &TaskId,
        approve: bool,
        feedback: Option<&str>,
    ) -> AppResult<Task> {
        let mut task = self.task(id)?;
        let waiting = task.status == TaskStatus::WaitingApproval
            && task
                .flow
                .as_ref()
                .and_then(|r| r.current())
                .is_some_and(|n| matches!(n.kind, NodeKind::Approval));
        if !waiting {
            return Err(AppError::NoFlowApproval(id.clone()));
        }
        let feedback = feedback.map(str::trim).filter(|f| !f.is_empty());
        if approve {
            if self.advance_flow(&mut task)? {
                self.flow_step(&mut task)?;
            }
        } else {
            let run = task.flow.as_mut().expect("checked above");
            let here = run.position.node.clone();
            let Some(back) = run.before(&here).map(str::to_owned) else {
                return Err(AppError::NoFlowApproval(id.clone()));
            };
            let text = feedback.map_or_else(
                || {
                    tr!(
                        "No lo aprobé: hazlo de otra forma.",
                        "I did not approve it: do it another way."
                    )
                },
                str::to_owned,
            );
            run.delivered.push(Delivery {
                node: here,
                role: tr!("Tú", "You"),
                text: text.clone(),
            });
            run.position.node = back;
            self.record(
                id,
                AgentEvent::UserMessage {
                    text,
                    images: Vec::new(),
                },
            )?;
            self.flow_step(&mut task)?;
        }
        self.events.task_updated(&task);
        Ok(task)
    }
}

#[cfg(test)]
mod tests {
    use crate::application::testing::{AgentCall, Fixture};
    use crate::application::{AppError, NewTask};
    use crate::domain::{AgentEvent, Phase, TaskMode, TaskStatus};

    fn request(f: &Fixture, prompt: &str) -> NewTask {
        NewTask {
            images: Vec::new(),
            project: f.dir("/code/tienda"),
            prompt: prompt.into(),
            mode: TaskMode::Project,
            model: None,
            plan_first: false,
            permissions: None,
        }
    }

    fn finish(f: &Fixture, summary: &str) {
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: summary.into(),
            duration_ms: None,
        });
    }

    fn setup() -> Fixture {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.hedy
            .use_account(&f.dir("/code/tienda"), "personal", None)
            .unwrap();
        f
    }

    #[test]
    fn the_classic_flow_plans_waits_for_you_then_implements() {
        let f = setup();
        let task = f
            .hedy
            .start_flow(request(&f, "Cobros con tarjeta"), "classic")
            .unwrap();
        assert!(task.workspace.is_some(), "the roles share a copy");
        // The architect reads and plans.
        let first = f.agents.last_request();
        assert_eq!(first.phase, Phase::Review, "read-only");
        assert!(first.prompt.contains("Cobros con tarjeta"));
        assert!(first.prompt.contains("arquitecto") || first.prompt.contains("architect"));
        finish(&f, "1. Cliente de pagos. 2. Webhook.");

        // Your OK.
        let waiting = f.hedy.task(&task.id).unwrap();
        assert_eq!(waiting.status, TaskStatus::WaitingApproval);
        let events = f.hedy.task_events(&task.id).unwrap();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::FlowApproval { .. }))
        );
        let before = f.agents.requests().len();
        f.hedy.answer_flow_approval(&task.id, true, None).unwrap();

        // The programmer gets the plan, in a session of its own.
        let requests = f.agents.requests();
        assert_eq!(requests.len(), before + 1);
        let programmer = requests.last().unwrap();
        assert_eq!(programmer.phase, Phase::Implement);
        assert!(
            programmer
                .prompt
                .contains("1. Cliente de pagos. 2. Webhook.")
        );
        assert!(programmer.resume.is_none());
        assert!(f.agents.calls().contains(&AgentCall::Stop));
        finish(&f, "Listo: 3 archivos.");

        let done = f.hedy.task(&task.id).unwrap();
        assert_eq!(done.status, TaskStatus::Review, "your review, as any task");
        assert_eq!(done.stages.len(), 2);
        assert_eq!(done.flow.unwrap().delivered.len(), 2);
    }

    #[test]
    fn not_approving_sends_the_plan_back_with_what_to_change() {
        let f = setup();
        let task = f
            .hedy
            .start_flow(request(&f, "Cobros con tarjeta"), "classic")
            .unwrap();
        finish(&f, "1. Todo en el cliente.");
        f.hedy
            .answer_flow_approval(&task.id, false, Some("Los pagos van en el servidor"))
            .unwrap();
        let again = f.agents.last_request();
        assert_eq!(again.phase, Phase::Review, "the architect again");
        assert!(again.prompt.contains("Los pagos van en el servidor"));
        assert!(again.prompt.contains("1. Todo en el cliente."));
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Running);
        // Nothing waits now.
        assert!(matches!(
            f.hedy.answer_flow_approval(&task.id, true, None),
            Err(AppError::NoFlowApproval(_))
        ));
    }

    #[test]
    fn flows_need_a_task_that_changes_code_and_steps_that_run_today() {
        let f = setup();
        let mut ask = request(&f, "Hola");
        ask.mode = TaskMode::Ask;
        assert!(matches!(
            f.hedy.start_flow(ask, "classic"),
            Err(AppError::InvalidFlow(_))
        ));
        // Conditions and loops come next (O3).
        assert!(matches!(
            f.hedy.start_flow(request(&f, "Hola"), "fix-loop"),
            Err(AppError::FlowNotYet)
        ));
        assert!(matches!(
            f.hedy.start_flow(request(&f, "Hola"), "nada"),
            Err(AppError::FlowNotFound(_))
        ));
        assert!(f.agents.requests().is_empty(), "nothing started");
    }
}
