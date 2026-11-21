//! Writing to the agent while it works: messages wait in a queue ("En cola")
//! and go as the next user turn of the same session when the agent's turn
//! ends. Never in the middle of a turn, never before the user answers a plan
//! or a permission card, and never to another account. When the task fails,
//! is stopped or reaches its limit, the queue is kept on hold and only the
//! user sends it (rule 3: nothing goes out by itself after a limit).

use super::tasks::current_stage;
use super::{AppError, AppResult, Hedy, NewImage};
use crate::domain::queue::join_queued;
use crate::domain::{HoldReason, LimitLevel, QueuedMessage, Task, TaskId, TaskStatus};

impl Hedy {
    /// The task's queue, oldest first.
    pub fn queued(&self, id: &TaskId) -> AppResult<Vec<QueuedMessage>> {
        Ok(self.store.queued(id)?)
    }

    /// A message from the user. While the agent's turn runs (or a card waits)
    /// it is queued; otherwise it goes now, resuming the session if needed.
    /// Returns what is left in the queue.
    pub fn queue_message(&self, id: &TaskId, text: &str) -> AppResult<Vec<QueuedMessage>> {
        self.queue_message_with(id, text, Vec::new())
    }

    /// Like [`Hedy::queue_message`], with images. Images only go with a
    /// message sent now: they never wait in the queue.
    pub fn queue_message_with(
        &self,
        id: &TaskId,
        text: &str,
        images: Vec<NewImage>,
    ) -> AppResult<Vec<QueuedMessage>> {
        let mut task = self.open_task(id)?;
        self.refuse_trial_run(id)?;
        let text = text.trim();
        if text.is_empty() {
            return Err(AppError::EmptyPrompt);
        }
        if !images.is_empty() {
            let account = self.account(current_stage(&task).account.as_str())?;
            self.check_images(&images, &account)?;
        }
        {
            let _queue = self.queue_lock.lock().unwrap();
            if self.turn_in_progress(&task) {
                if !images.is_empty() {
                    return Err(AppError::ImagesWhileQueued);
                }
                self.store.add_queued(id, text, self.now_secs())?;
                return self.queue_changed(id);
            }
        }
        let attachments = self.store_images(id, &images)?;
        self.deliver(&mut task, text, attachments)?;
        self.queued(id)
    }

    /// Changes a message that has not gone yet.
    pub fn edit_queued(&self, message: i64, text: &str) -> AppResult<Vec<QueuedMessage>> {
        let text = text.trim();
        if text.is_empty() {
            return Err(AppError::EmptyPrompt);
        }
        let _queue = self.queue_lock.lock().unwrap();
        let queued = self.queued_message(message)?;
        self.store.update_queued(message, text)?;
        self.queue_changed(&queued.task)
    }

    /// Takes a message out of the queue before it goes.
    pub fn remove_queued(&self, message: i64) -> AppResult<Vec<QueuedMessage>> {
        let _queue = self.queue_lock.lock().unwrap();
        let queued = self.queued_message(message)?;
        self.store.delete_queued(&[message])?;
        self.queue_changed(&queued.task)
    }

    /// The user sends the queue: messages on hold are released and, unless
    /// the agent is in the middle of a turn, everything goes now (to the
    /// task's current account, resuming its session if needed).
    pub fn send_queued_now(&self, id: &TaskId) -> AppResult<Vec<QueuedMessage>> {
        let mut task = self.open_task(id)?;
        let messages = {
            let _queue = self.queue_lock.lock().unwrap();
            self.store.hold_queue(id, None)?;
            let messages = self.store.queued(id)?;
            if messages.is_empty() || self.turn_in_progress(&task) {
                return self.queue_changed(id);
            }
            self.store
                .delete_queued(&messages.iter().map(|m| m.id).collect::<Vec<_>>())?;
            messages
        };
        let sent = self.deliver(&mut task, &join_queued(&messages), Vec::new());
        if sent.is_err() {
            // Not lost: back in the queue, on hold.
            for m in &messages {
                self.store.add_queued(id, &m.text, m.created_at)?;
            }
            self.store.hold_queue(id, Some(HoldReason::Failed))?;
        }
        self.queue_changed(id)?;
        sent?;
        self.queued(id)
    }

    /// The agent is working on a turn, or a card waits for the user: a new
    /// message must wait.
    pub(super) fn turn_in_progress(&self, task: &Task) -> bool {
        matches!(
            task.status,
            TaskStatus::Running | TaskStatus::WaitingApproval
        ) && self.is_running(&task.id)
    }

    /// At the end of a successful turn: sends the queue (messages not on
    /// hold) as the next user turn of the same session. `false` when there
    /// was nothing to send, or the decision on a card comes first.
    pub(super) fn deliver_queue_at_turn_end(&self, task: &mut Task) -> AppResult<bool> {
        let waiting = task.status == TaskStatus::WaitingApproval
            || self
                .live
                .lock()
                .unwrap()
                .get(&task.id)
                .is_none_or(|running| !running.pending.is_empty());
        if waiting {
            return Ok(false);
        }
        let messages: Vec<QueuedMessage> = {
            let _queue = self.queue_lock.lock().unwrap();
            let messages: Vec<_> = self
                .store
                .queued(&task.id)?
                .into_iter()
                .filter(|m| m.held.is_none())
                .collect();
            if messages.is_empty() {
                return Ok(false);
            }
            self.store
                .delete_queued(&messages.iter().map(|m| m.id).collect::<Vec<_>>())?;
            messages
        };
        self.queue_changed(&task.id)?;
        self.deliver(task, &join_queued(&messages), Vec::new())?;
        Ok(true)
    }

    /// Whether this session's account reached its limit.
    pub(super) fn limit_reached_now(&self, id: &TaskId) -> bool {
        self.live
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|running| running.warned == Some(LimitLevel::Reached))
    }

    /// Puts the queue on hold, telling the UI when anything changed.
    pub(super) fn hold_queue(&self, id: &TaskId, reason: HoldReason) {
        let _queue = self.queue_lock.lock().unwrap();
        let Ok(queue) = self.store.queued(id) else {
            return;
        };
        if queue.iter().any(|m| m.held.is_none()) && self.store.hold_queue(id, Some(reason)).is_ok()
        {
            let _ = self.queue_changed(id);
        }
    }

    /// Accepted and discarded tasks take no more messages.
    pub(super) fn clear_queue(&self, id: &TaskId) -> AppResult<()> {
        let _queue = self.queue_lock.lock().unwrap();
        let queue = self.store.queued(id)?;
        if !queue.is_empty() {
            self.store
                .delete_queued(&queue.iter().map(|m| m.id).collect::<Vec<_>>())?;
            self.queue_changed(id)?;
        }
        Ok(())
    }

    fn open_task(&self, id: &TaskId) -> AppResult<Task> {
        let task = self.task(id)?;
        if task.status.is_closed() {
            return Err(AppError::TaskClosed(id.clone()));
        }
        Ok(task)
    }

    fn queued_message(&self, id: i64) -> AppResult<QueuedMessage> {
        let message = self
            .store
            .queued_message(id)?
            .ok_or(AppError::QueuedMessageNotFound(id))?;
        self.open_task(&message.task)?;
        Ok(message)
    }

    fn queue_changed(&self, id: &TaskId) -> AppResult<Vec<QueuedMessage>> {
        let queue = self.store.queued(id)?;
        self.events.queue_changed(id, &queue);
        Ok(queue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::NewTask;
    use crate::application::testing::{AgentCall, Fixture};
    use crate::domain::{AgentEvent, Decision, TaskMode};

    fn running_task(f: &Fixture, plan_first: bool) -> Task {
        f.add_claude("Personal");
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        f.hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/agenda"),
                prompt: "Recordatorios".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first,
                permissions: None,
            })
            .unwrap()
    }

    fn finished(ok: bool) -> AgentEvent {
        AgentEvent::Finished {
            ok,
            summary: "Listo".into(),
            duration_ms: None,
        }
    }

    fn sends(f: &Fixture) -> Vec<String> {
        f.agents
            .calls()
            .into_iter()
            .filter_map(|c| match c {
                AgentCall::Send(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    fn texts(queue: &[QueuedMessage]) -> Vec<&str> {
        queue.iter().map(|m| m.text.as_str()).collect()
    }

    #[test]
    fn messages_wait_for_the_end_of_the_turn_and_go_together() {
        let f = Fixture::new();
        let task = running_task(&f, false);
        f.hedy.queue_message(&task.id, "y los tests").unwrap();
        let queue = f.hedy.queue_message(&task.id, " usa pytest ").unwrap();
        assert_eq!(texts(&queue), vec!["y los tests", "usa pytest"]);
        assert!(sends(&f).is_empty(), "nothing goes mid-turn");

        f.agents.emit(finished(true));
        assert_eq!(sends(&f), vec!["y los tests\n\nusa pytest"]);
        assert!(f.hedy.queued(&task.id).unwrap().is_empty());
        let task = f.hedy.task(&task.id).unwrap();
        assert_eq!(task.status, TaskStatus::Running, "the agent goes on");
        assert_eq!(f.agents.requests().len(), 1, "same session");
        let events = f.hedy.task_events(&task.id).unwrap();
        assert_eq!(
            events.last(),
            Some(&AgentEvent::UserMessage {
                images: Vec::new(),
                text: "y los tests\n\nusa pytest".into()
            })
        );
        assert_eq!(f.notices.queue_lengths(), vec![1, 2, 0]);
        assert!(
            f.notices.titles().is_empty(),
            "no 'finished' notice while it goes on"
        );
    }

    #[test]
    fn an_idle_task_gets_the_message_right_away() {
        let f = Fixture::new();
        let task = running_task(&f, false);
        f.agents.emit(finished(true));
        let queue = f.hedy.queue_message(&task.id, "sigue").unwrap();
        assert!(queue.is_empty());
        assert_eq!(sends(&f), vec!["sigue"]);
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Running);
    }

    #[test]
    fn queued_messages_can_be_edited_or_removed_until_they_go() {
        let f = Fixture::new();
        let task = running_task(&f, false);
        let first = f.hedy.queue_message(&task.id, "uno").unwrap()[0].clone();
        let second = f.hedy.queue_message(&task.id, "dos").unwrap()[1].clone();
        f.hedy.edit_queued(first.id, "uno bis").unwrap();
        let queue = f.hedy.remove_queued(second.id).unwrap();
        assert_eq!(texts(&queue), vec!["uno bis"]);
        assert!(matches!(
            f.hedy.edit_queued(first.id, " "),
            Err(AppError::EmptyPrompt)
        ));

        f.agents.emit(finished(true));
        assert_eq!(sends(&f), vec!["uno bis"]);
        assert!(matches!(
            f.hedy.remove_queued(first.id),
            Err(AppError::QueuedMessageNotFound(_))
        ));
    }

    #[test]
    fn the_plan_decision_comes_before_the_queue() {
        let f = Fixture::new();
        let task = running_task(&f, true);
        f.agents.emit(AgentEvent::Plan {
            text: "1. Hacer".into(),
            request_id: Some("p1".into()),
        });
        f.hedy.queue_message(&task.id, "y documenta").unwrap();
        // A turn that ends while the plan waits does not deliver the queue.
        f.agents.emit(finished(true));
        assert!(sends(&f).is_empty());

        f.hedy.answer_plan(&task.id, "p1", true, None).unwrap();
        assert_eq!(f.agents.calls(), vec![AgentCall::Answer("p1".into(), true)]);
        f.agents.emit(finished(true));
        assert_eq!(sends(&f), vec!["y documenta"]);
    }

    #[test]
    fn permission_cards_are_answered_before_the_queue() {
        let f = Fixture::new();
        let task = running_task(&f, false);
        f.agents.emit(AgentEvent::PermissionRequest {
            request_id: "r1".into(),
            tool: "Bash".into(),
            summary: "pytest".into(),
            input: serde_json::json!({"command": "pytest"}),
        });
        let queue = f.hedy.queue_message(&task.id, "luego el README").unwrap();
        assert_eq!(queue.len(), 1, "queued while the card waits");
        f.hedy
            .answer_permission(&task.id, "r1", Decision::Allow)
            .unwrap();
        assert!(sends(&f).is_empty());
        f.agents.emit(finished(true));
        assert_eq!(sends(&f), vec!["luego el README"]);
    }

    #[test]
    fn a_failed_or_stopped_task_keeps_its_queue_on_hold() {
        let f = Fixture::new();
        let task = running_task(&f, false);
        f.hedy.queue_message(&task.id, "uno").unwrap();
        f.agents.emit(finished(false));
        assert!(sends(&f).is_empty());
        let queue = f.hedy.queued(&task.id).unwrap();
        assert_eq!(queue[0].held, Some(HoldReason::Failed));
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Failed);

        // The user decides: it goes to the same session.
        let left = f.hedy.send_queued_now(&task.id).unwrap();
        assert!(left.is_empty());
        assert_eq!(sends(&f), vec!["uno"]);

        f.hedy.queue_message(&task.id, "dos").unwrap();
        f.hedy.stop_task(&task.id).unwrap();
        let queue = f.hedy.queued(&task.id).unwrap();
        assert_eq!(queue[0].held, Some(HoldReason::Stopped));
        // Sending now resumes the stopped session.
        f.hedy.send_queued_now(&task.id).unwrap();
        assert_eq!(f.agents.requests().len(), 2);
        assert_eq!(f.agents.last_request().prompt, "dos");
    }

    #[test]
    fn after_a_limit_nothing_is_sent_by_itself_nor_to_another_account() {
        let f = Fixture::new();
        let task = running_task(&f, false);
        f.add_kimi("Kimi");
        f.hedy.set_api_key("kimi", "sk").unwrap();
        f.hedy.queue_message(&task.id, "sigue").unwrap();
        f.agents.emit(AgentEvent::RateLimit {
            status: "rejected".into(),
            utilization: None,
            resets_at: Some(99),
        });
        f.agents.emit(finished(true));
        assert!(sends(&f).is_empty(), "rule 3");
        assert_eq!(
            f.hedy.queued(&task.id).unwrap()[0].held,
            Some(HoldReason::Limit)
        );

        // A handoff (the user's choice) does not take the held queue along.
        f.hedy.handoff_task(&task.id, "kimi", None).unwrap();
        f.agents.emit(finished(true));
        assert!(sends(&f).is_empty());
        assert_eq!(f.hedy.queued(&task.id).unwrap().len(), 1);
    }

    #[test]
    fn a_queued_message_goes_before_the_next_phase() {
        use crate::domain::{Assignment, Phase, PhasePlan};
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_kimi("Kimi");
        f.hedy.set_api_key("kimi", "sk").unwrap();
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        f.hedy
            .set_project_phases(
                &f.dir("/code/agenda"),
                PhasePlan::from([(
                    Phase::Tests,
                    Assignment {
                        account: crate::domain::AccountId::new("kimi").unwrap(),
                        model: None,
                    },
                )]),
            )
            .unwrap();
        let task = f
            .hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/agenda"),
                prompt: "Hola".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: false,
                permissions: None,
            })
            .unwrap();
        f.hedy.queue_message(&task.id, "también el README").unwrap();
        f.agents.emit(finished(true));
        assert_eq!(sends(&f), vec!["también el README"]);
        assert_eq!(f.agents.requests().len(), 1, "same phase, same session");
        f.agents.emit(finished(true));
        assert_eq!(f.agents.requests().len(), 2, "then the tests phase");
    }

    #[test]
    fn closing_a_task_drops_its_queue_and_takes_no_more() {
        let f = Fixture::new();
        let task = running_task(&f, false);
        f.hedy.queue_message(&task.id, "uno").unwrap();
        f.hedy.discard_task(&task.id).unwrap();
        assert!(f.hedy.queued(&task.id).unwrap().is_empty());
        assert!(matches!(
            f.hedy.queue_message(&task.id, "dos"),
            Err(AppError::TaskClosed(_))
        ));
    }
}
