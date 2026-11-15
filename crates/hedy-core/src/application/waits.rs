//! "Esperar" on a limit card, for real: the decision is stored in the task
//! with the time the limit renews, and when that time comes Hedy resumes the
//! task **on the same account** (rule 3: never another one), sending what the
//! user left queued or a short "continue" turn, and tells the user.
//!
//! Only while Hedy runs: the app calls [`Hedy::resume_due_waits`] on a timer.
//! A reset that passed while Hedy was closed is not acted on at start-up
//! ([`Hedy::mark_overdue_waits`]): the task shows "Ya puedes continuar" and
//! the user resumes it, so nothing spends tokens without the user there.

use super::tasks::current_stage;
use super::{AppError, AppResult, Hedy};
use crate::domain::prompts;
use crate::domain::queue::join_queued;
use crate::domain::{
    AgentEvent, HoldReason, LimitLevel, LimitWait, NoticeKind, QueuedMessage, Task, TaskId,
};

impl Hedy {
    /// "Esperar": remembers that the user waits for the task's account to
    /// renew its limit. Returns the task with its wait.
    pub fn wait_for_limit(&self, id: &TaskId) -> AppResult<Task> {
        let mut task = self.task(id)?;
        if task.status.is_closed() {
            return Err(AppError::TaskClosed(id.clone()));
        }
        task.wait = Some(LimitWait {
            account: task.account.clone(),
            until: self.limit_resets_at(&task)?,
            since: self.now_secs(),
            ready: false,
        });
        self.store.save_task(&task)?;
        self.events.task_updated(&task);
        Ok(task)
    }

    /// The user no longer waits: the limit card offers its options again.
    pub fn cancel_wait(&self, id: &TaskId) -> AppResult<Task> {
        let mut task = self.task(id)?;
        if task.wait.take().is_some() {
            self.store.save_task(&task)?;
            self.events.task_updated(&task);
        }
        Ok(task)
    }

    /// "Continuar": the user resumes the task now, on its account.
    pub fn resume_after_limit(&self, id: &TaskId) -> AppResult<Task> {
        let mut task = self.task(id)?;
        if task.status.is_closed() {
            return Err(AppError::TaskClosed(id.clone()));
        }
        self.resume_on_same_account(&mut task)?;
        Ok(task)
    }

    /// Called on a timer while the app runs: resumes every task whose limit
    /// renewed. A task that cannot resume keeps its wait, marked ready, so
    /// the user sees "Ya puedes continuar". Returns the tasks resumed.
    pub fn resume_due_waits(&self) -> AppResult<Vec<TaskId>> {
        let now = self.now_secs();
        let mut resumed = Vec::new();
        for mut task in self.store.waiting_tasks()? {
            if !task.wait.as_ref().is_some_and(|w| w.due(now)) {
                continue;
            }
            if task.status.is_closed() {
                self.cancel_wait(&task.id)?;
                continue;
            }
            let wait = task.wait.clone();
            match self.resume_on_same_account(&mut task) {
                Ok(true) => {
                    self.notify(&task, NoticeKind::LimitRenewed { resumed: true });
                    resumed.push(task.id.clone());
                }
                Ok(false) => {}
                Err(error) => {
                    let _ = self.record(
                        &task.id,
                        AgentEvent::Error {
                            message: error.to_string(),
                        },
                    );
                    let mut task = self.task(&task.id)?;
                    task.wait = wait.map(|w| LimitWait { ready: true, ..w });
                    self.store.save_task(&task)?;
                    self.events.task_updated(&task);
                    self.notify(&task, NoticeKind::LimitRenewed { resumed: false });
                }
            }
        }
        Ok(resumed)
    }

    /// At start-up: waits whose reset passed while Hedy was closed become
    /// "Ya puedes continuar"; nothing is sent by itself. Returns those tasks.
    pub fn mark_overdue_waits(&self) -> AppResult<Vec<TaskId>> {
        let now = self.now_secs();
        let mut ready = Vec::new();
        for mut task in self.store.waiting_tasks()? {
            let Some(wait) = task.wait.as_mut().filter(|w| w.due(now)) else {
                continue;
            };
            wait.ready = true;
            self.store.save_task(&task)?;
            self.events.task_updated(&task);
            if !task.status.is_closed() {
                self.notify(&task, NoticeKind::LimitRenewed { resumed: false });
            }
            ready.push(task.id.clone());
        }
        Ok(ready)
    }

    /// When the account's limit renews, from the latest limit report of this
    /// account in the task, else its plan usage (the latest renewal among the
    /// windows at their limit), else its last rate-limit report. Only future
    /// times count; `None` when unknown.
    fn limit_resets_at(&self, task: &Task) -> AppResult<Option<i64>> {
        let now = self.now_secs();
        let future = |at: Option<i64>| at.filter(|at| *at > now);
        let reported = self
            .store
            .events(&task.id)?
            .into_iter()
            .rev()
            .find_map(|event| match event {
                AgentEvent::LimitWarning {
                    account,
                    level: LimitLevel::Reached,
                    resets_at,
                    ..
                } if account == task.account.as_str() => Some(resets_at),
                _ => None,
            })
            .flatten();
        if let Some(at) = future(reported) {
            return Ok(Some(at));
        }
        let account = self.account(task.account.as_str())?;
        let from_usage = self.plan_usage_of(&account).ok().and_then(|state| {
            state
                .usage?
                .windows
                .iter()
                .filter(|w| w.percent >= 100.0)
                .filter_map(|w| future(w.resets_at))
                .max()
        });
        if from_usage.is_some() {
            return Ok(from_usage);
        }
        Ok(future(
            self.store
                .rate_limit(&task.account)?
                .and_then(|limit| limit.resets_at),
        ))
    }

    /// Sends the held queue (or a "continue" turn) as the next turn of the
    /// task's session, on the account it waited for. `false` when the task
    /// moved to another account meanwhile (a handoff): nothing is sent.
    fn resume_on_same_account(&self, task: &mut Task) -> AppResult<bool> {
        let id = task.id.clone();
        let wait = task.wait.take();
        self.store.save_task(task)?;
        // Rule 3: only ever the account whose limit the user waited for.
        if let Some(wait) = &wait
            && (wait.account != task.account || current_stage(task).account != wait.account)
        {
            self.events.task_updated(task);
            return Ok(false);
        }
        // The user chose to go on: a new limit in this session warns again.
        if let Some(running) = self.live.lock().unwrap().get_mut(&id) {
            running.warned = None;
        }
        let messages: Vec<QueuedMessage> = {
            let _queue = self.queue_lock.lock().unwrap();
            let messages = self.store.queued(&id)?;
            if !messages.is_empty() {
                self.store
                    .delete_queued(&messages.iter().map(|m| m.id).collect::<Vec<_>>())?;
            }
            messages
        };
        let text = if messages.is_empty() {
            prompts::continue_after_limit()
        } else {
            join_queued(&messages)
        };
        if self.turn_in_progress(task) {
            // Never mid-turn: it goes when the turn ends.
            let _queue = self.queue_lock.lock().unwrap();
            self.store.add_queued(&id, &text, self.now_secs())?;
            self.events.queue_changed(&id, &self.store.queued(&id)?);
            self.events.task_updated(task);
            return Ok(true);
        }
        if !messages.is_empty() {
            self.events.queue_changed(&id, &[]);
        }
        if let Err(error) = self.deliver(task, &text) {
            // Not lost: back in the queue, on hold.
            let _queue = self.queue_lock.lock().unwrap();
            for m in &messages {
                self.store.add_queued(&id, &m.text, m.created_at)?;
            }
            self.store.hold_queue(&id, Some(HoldReason::Limit))?;
            self.events.queue_changed(&id, &self.store.queued(&id)?);
            return Err(error);
        }
        self.events.task_updated(task);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::NewTask;
    use crate::application::ports::{PlanUsageRecord, TaskRepository, UsageRepository};
    use crate::application::testing::{AgentCall, Fixture};
    use crate::domain::{PlanUsage, TaskMode, TaskStatus, UsageSource, UsageWindow, WindowKind};
    use crate::i18n::{Language, with_language};

    const NOW: i64 = 1_700_000_000;

    fn task_at_limit(f: &Fixture, resets_at: Option<i64>) -> Task {
        task_at_limit_with(f, resets_at, None)
    }

    /// `queued` is written while the agent works, before the limit stops it.
    fn task_at_limit_with(f: &Fixture, resets_at: Option<i64>, queued: Option<&str>) -> Task {
        f.add_claude("Personal");
        f.add_kimi("Kimi");
        f.fs.add_git_root("/code/moira");
        f.hedy
            .use_account(&f.dir("/code/moira"), "personal", None)
            .unwrap();
        let task = f
            .hedy
            .start_task(NewTask {
                project: f.dir("/code/moira"),
                prompt: "Recordatorios".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: false,
            })
            .unwrap();
        f.agents.emit(AgentEvent::SessionStarted {
            session_id: "s-1".into(),
            model: "opus".into(),
        });
        f.agents.emit(AgentEvent::RateLimit {
            status: "rejected".into(),
            utilization: None,
            resets_at,
        });
        if let Some(text) = queued {
            f.hedy.queue_message(&task.id, text).unwrap();
        }
        f.agents.emit(AgentEvent::Finished {
            ok: false,
            summary: String::new(),
            duration_ms: None,
        });
        f.hedy.task(&task.id).unwrap()
    }

    /// The fake clock also ticks a second on every reading: keep margins.
    fn advance(f: &Fixture, secs: i64) {
        f.clock.advance(secs);
    }

    /// A reset far enough ahead of the fixture's own clock ticks.
    const LATER: i64 = NOW + 10_000;

    #[test]
    fn waiting_is_stored_with_the_reset_of_the_limit_report() {
        let f = Fixture::new();
        let task = task_at_limit(&f, Some(LATER));
        let waiting = f.hedy.wait_for_limit(&task.id).unwrap();
        let wait = waiting.wait.unwrap();
        assert_eq!(wait.account.as_str(), "personal");
        assert_eq!(wait.until, Some(LATER));
        assert!(!wait.ready);
        // Stored: it survives navigation and restarts.
        assert_eq!(f.store.task(&task.id).unwrap().unwrap().wait, Some(wait));
        assert!(
            f.notices
                .updated()
                .iter()
                .any(|t| t.id == task.id && t.wait.is_some())
        );

        let cancelled = f.hedy.cancel_wait(&task.id).unwrap();
        assert_eq!(cancelled.wait, None);
        assert_eq!(f.hedy.task(&task.id).unwrap().wait, None);
    }

    #[test]
    fn without_a_reported_reset_the_plan_usage_says_when() {
        let f = Fixture::new();
        let task = task_at_limit(&f, None);
        let window = |kind, percent: f64, resets_at| UsageWindow {
            kind,
            minutes: None,
            model: None,
            percent,
            resets_text: None,
            resets_at: Some(resets_at),
        };
        f.store
            .set_plan_usage(
                &task.account,
                &PlanUsageRecord {
                    usage: Some(PlanUsage {
                        windows: vec![
                            window(WindowKind::Session, 100.0, NOW + 6_000),
                            window(WindowKind::Week, 100.0, NOW + 72_000),
                            window(WindowKind::Day, 40.0, NOW + 99_999),
                        ],
                        checked_at: NOW,
                        source: UsageSource::ClaudeUsage,
                        balance: None,
                    }),
                    attempted_at: NOW,
                    problem: None,
                },
            )
            .unwrap();
        let wait = f.hedy.wait_for_limit(&task.id).unwrap().wait.unwrap();
        assert_eq!(
            wait.until,
            Some(NOW + 72_000),
            "both full windows must renew"
        );
    }

    #[test]
    fn an_unknown_reset_waits_for_the_user() {
        let f = Fixture::new();
        let task = task_at_limit(&f, None);
        let wait = f.hedy.wait_for_limit(&task.id).unwrap().wait.unwrap();
        assert_eq!(wait.until, None);
        advance(&f, 10 * 24 * 3600);
        assert!(f.hedy.resume_due_waits().unwrap().is_empty());
        assert!(f.hedy.task(&task.id).unwrap().wait.is_some());
    }

    #[test]
    fn at_the_reset_hedy_resumes_on_the_same_account_with_the_held_queue() {
        let f = Fixture::new();
        let task = task_at_limit_with(&f, Some(LATER), Some("Revisa también el asunto"));
        assert_eq!(
            f.hedy.queued(&task.id).unwrap()[0].held,
            Some(HoldReason::Limit)
        );
        f.hedy.wait_for_limit(&task.id).unwrap();
        let starts = f.agents.requests().len();

        advance(&f, 5_000);
        assert!(f.hedy.resume_due_waits().unwrap().is_empty());

        advance(&f, 5_000);
        assert_eq!(f.hedy.resume_due_waits().unwrap(), vec![task.id.clone()]);
        let resumed = f.hedy.task(&task.id).unwrap();
        assert_eq!(resumed.wait, None);
        assert_eq!(resumed.status, TaskStatus::Running);
        assert_eq!(resumed.account.as_str(), "personal");
        assert!(f.hedy.queued(&task.id).unwrap().is_empty());
        // The message went to the same account: the live session or a resumed one.
        let sent_live = f
            .agents
            .calls()
            .contains(&AgentCall::Send("Revisa también el asunto".into()));
        let resumed_session = f.agents.requests().len() > starts && {
            let request = f.agents.last_request();
            request.prompt == "Revisa también el asunto" && request.resume.as_deref() == Some("s-1")
        };
        assert!(sent_live || resumed_session);
        assert!(
            f.notices
                .kinds()
                .contains(&NoticeKind::LimitRenewed { resumed: true })
        );
        // Done once.
        assert!(f.hedy.resume_due_waits().unwrap().is_empty());
    }

    #[test]
    fn with_nothing_queued_it_sends_a_continue_turn() {
        let f = Fixture::new();
        let task = task_at_limit(&f, Some(LATER));
        f.hedy.wait_for_limit(&task.id).unwrap();
        advance(&f, 10_000);
        with_language(Language::Spanish, || f.hedy.resume_due_waits().unwrap());
        let events = f.hedy.task_events(&task.id).unwrap();
        assert!(events.contains(&AgentEvent::UserMessage {
            text: "Tu límite se renovó. Continúa con la tarea donde la dejaste.".into()
        }));
    }

    #[test]
    fn a_handoff_meanwhile_ends_the_wait_and_nothing_goes_to_the_old_account() {
        let f = Fixture::new();
        let task = task_at_limit(&f, Some(LATER));
        f.hedy.wait_for_limit(&task.id).unwrap();
        f.hedy.set_api_key("kimi", "sk-test").unwrap();
        let moved = f.hedy.handoff_task(&task.id, "kimi", None).unwrap();
        assert_eq!(moved.wait, None);
        advance(&f, 10_000);
        assert!(f.hedy.resume_due_waits().unwrap().is_empty());
        assert_eq!(f.hedy.task(&task.id).unwrap().account.as_str(), "kimi");
    }

    #[test]
    fn a_wait_whose_account_changed_resumes_nothing() {
        let f = Fixture::new();
        let task = task_at_limit(&f, Some(LATER));
        let mut waiting = f.hedy.wait_for_limit(&task.id).unwrap();
        // Tampered or stale: the task's account is not the one waited for.
        waiting.wait.as_mut().unwrap().account = crate::domain::AccountId::new("kimi").unwrap();
        f.store.save_task(&waiting).unwrap();
        let before = f.hedy.task_events(&task.id).unwrap().len();
        advance(&f, 10_000);
        assert!(f.hedy.resume_due_waits().unwrap().is_empty());
        assert_eq!(f.hedy.task(&task.id).unwrap().wait, None);
        assert_eq!(f.hedy.task_events(&task.id).unwrap().len(), before);
    }

    #[test]
    fn a_reset_passed_while_closed_shows_continue_and_sends_nothing() {
        let f = Fixture::new();
        let task = task_at_limit(&f, Some(LATER));
        f.hedy.wait_for_limit(&task.id).unwrap();
        let before = f.hedy.task_events(&task.id).unwrap().len();
        advance(&f, 20_000);

        assert_eq!(f.hedy.mark_overdue_waits().unwrap(), vec![task.id.clone()]);
        let wait = f.hedy.task(&task.id).unwrap().wait.unwrap();
        assert!(wait.ready);
        assert!(
            f.notices
                .kinds()
                .contains(&NoticeKind::LimitRenewed { resumed: false })
        );
        // The timer does not act on it either.
        assert!(f.hedy.resume_due_waits().unwrap().is_empty());
        assert_eq!(f.hedy.task_events(&task.id).unwrap().len(), before);

        // The user presses "Continuar".
        let resumed = f.hedy.resume_after_limit(&task.id).unwrap();
        assert_eq!(resumed.wait, None);
        assert_eq!(resumed.status, TaskStatus::Running);
    }

    #[test]
    fn writing_to_the_agent_ends_the_wait() {
        let f = Fixture::new();
        let task = task_at_limit(&f, Some(LATER));
        f.hedy.wait_for_limit(&task.id).unwrap();
        f.hedy.send_queued_now(&task.id).unwrap();
        f.hedy.queue_message(&task.id, "sigue").unwrap();
        assert_eq!(f.hedy.task(&task.id).unwrap().wait, None);
    }

    #[test]
    fn a_failed_resume_keeps_the_wait_for_the_user() {
        let f = Fixture::new();
        let task = task_at_limit_with(&f, Some(LATER), Some("Revisa el asunto"));
        f.hedy.wait_for_limit(&task.id).unwrap();
        f.hedy.stop_task(&task.id).unwrap();
        f.agents.fail_next_start();
        advance(&f, 10_000);
        assert!(f.hedy.resume_due_waits().unwrap().is_empty());
        let task = f.hedy.task(&task.id).unwrap();
        assert!(task.wait.unwrap().ready);
        let queue = f.hedy.queued(&task.id).unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].held, Some(HoldReason::Limit));
    }

    #[test]
    fn closed_tasks_cannot_wait() {
        let f = Fixture::new();
        let task = task_at_limit(&f, Some(LATER));
        f.hedy.discard_task(&task.id).unwrap();
        assert!(matches!(
            f.hedy.wait_for_limit(&task.id),
            Err(AppError::TaskClosed(_))
        ));
    }
}
