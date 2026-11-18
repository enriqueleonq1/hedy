//! In-memory fakes of every port, for use-case tests.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use super::ports::{
    AccountRepository, AgentBackend, AgentLauncher, AgentRequest, AgentSession, CapturedOutput,
    Clock, CommandRepository, EventSink, FileSystem, Git, MemoryRepository, MergeOutcome, NewNote,
    NewSkill, NoteRevision, PlanUsageRecord, PortError, PortResult, ProcessRunner,
    ProjectRepository, QueueRepository, RateLimit, SecretStore, SettingsRepository,
    SkillRepository, SkillRevision, StoreHealth, TaskEvents, TaskRepository, TemplateRecord,
    TemplateRepository, ThemeStore, UsageProbe, UsageRecord, UsageRepository,
};
use super::{Hedy, NewAccount, Ports};
use crate::domain::recall::NoteUse;

mod ship;
mod sync;
use crate::domain::{
    Account, AccountId, AgentEvent, Category, Commit, FileDiff, HoldReason, ImageSupport,
    KnownCommands, LaunchSpec, Note, NoteStatus, Notice, NoticeKind, Project, PromptImage,
    Provider, QueuedMessage, Skill, SkillStatus, Task, TaskId, TaskStatus, UsageSample,
    theme::CustomTheme,
};
pub use ship::{FakeHost, FakeSource};
pub use sync::{FakeSyncRemote, SyncServer};

#[derive(Default)]
pub struct MemoryStore {
    accounts: Mutex<Vec<Account>>,
    projects: Mutex<Vec<Project>>,
    settings: Mutex<BTreeMap<String, String>>,
    tasks: Mutex<Vec<Task>>,
    events: Mutex<HashMap<TaskId, Vec<(i64, AgentEvent)>>>,
    usage: Mutex<Vec<UsageRecord>>,
    limits: Mutex<HashMap<AccountId, RateLimit>>,
    plan_usage: Mutex<HashMap<AccountId, PlanUsageRecord>>,
    notes: Mutex<Vec<Note>>,
    /// (note, task, at).
    uses: Mutex<Vec<(i64, Option<TaskId>, i64)>>,
    dismissed: Mutex<Vec<(i64, i64)>>,
    skills: Mutex<Vec<Skill>>,
    integrity: Mutex<Vec<String>>,
    queue: Mutex<Vec<QueuedMessage>>,
    templates: Mutex<Vec<TemplateRecord>>,
    commands: Mutex<Vec<(AccountId, Option<PathBuf>, KnownCommands)>>,
    sync: Mutex<sync::SyncTables>,
}

impl MemoryStore {
    fn project_tasks(&self, project: &Path) -> HashSet<TaskId> {
        self.tasks
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.project == project)
            .map(|t| t.id.clone())
            .collect()
    }

    /// What the next integrity check finds.
    pub fn corrupt(&self, problem: &str) {
        self.integrity.lock().unwrap().push(problem.into());
    }
}

impl StoreHealth for MemoryStore {
    fn data_dir(&self) -> Option<PathBuf> {
        Some("/data".into())
    }
    fn integrity_problems(&self) -> PortResult<Vec<String>> {
        Ok(self.integrity.lock().unwrap().clone())
    }
}

fn in_scope(note: &Note, project: Option<&Path>, with_global: bool) -> bool {
    match (&note.project, project) {
        (None, Some(_)) => with_global,
        (None, None) => true,
        (Some(p), Some(q)) => p == q,
        (Some(_), None) => false,
    }
}

impl MemoryRepository for MemoryStore {
    fn add_note(&self, n: &NewNote) -> PortResult<Note> {
        let mut notes = self.notes.lock().unwrap();
        let note = Note {
            id: notes.iter().map(|n| n.id).max().unwrap_or(0) + 1,
            project: n.project.clone(),
            text: n.text.clone(),
            category: n.category,
            author: n.author.clone(),
            status: n.status,
            created_at: n.created_at,
            uses: 0,
            last_used: None,
            topic: n.topic.clone(),
            updated_at: None,
        };
        notes.push(note.clone());
        Ok(note)
    }
    fn note(&self, id: i64) -> PortResult<Option<Note>> {
        Ok(self
            .notes
            .lock()
            .unwrap()
            .iter()
            .find(|n| n.id == id)
            .cloned())
    }
    fn update_note(&self, id: i64, text: &str, category: Category) -> PortResult<()> {
        if let Some(n) = self.notes.lock().unwrap().iter_mut().find(|n| n.id == id) {
            n.text = text.into();
            n.category = category;
        }
        Ok(())
    }
    fn revise_note(&self, id: i64, r: &NoteRevision) -> PortResult<()> {
        if let Some(n) = self.notes.lock().unwrap().iter_mut().find(|n| n.id == id) {
            n.text = r.text.clone();
            n.category = r.category;
            n.author = r.author.clone();
            n.topic = r.topic.clone();
            n.updated_at = Some(r.updated_at);
        }
        Ok(())
    }
    fn set_note_status(&self, id: i64, status: NoteStatus) -> PortResult<()> {
        if let Some(n) = self.notes.lock().unwrap().iter_mut().find(|n| n.id == id) {
            n.status = status;
        }
        Ok(())
    }
    fn delete_note(&self, id: i64) -> PortResult<()> {
        self.notes.lock().unwrap().retain(|n| n.id != id);
        Ok(())
    }
    fn notes(&self, project: Option<&Path>, with_global: bool) -> PortResult<Vec<Note>> {
        Ok(self
            .notes
            .lock()
            .unwrap()
            .iter()
            .filter(|n| in_scope(n, project, with_global))
            .cloned()
            .collect())
    }
    fn search_notes(
        &self,
        project: Option<&Path>,
        with_global: bool,
        query: &str,
    ) -> PortResult<Vec<Note>> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        Ok(self
            .notes(project, with_global)?
            .into_iter()
            .filter(|n| {
                let haystack = format!("{} {}", n.text, n.topic.as_deref().unwrap_or(""));
                words.is_empty() || words.iter().any(|w| haystack.to_lowercase().contains(w))
            })
            .collect())
    }
    fn record_uses(&self, ids: &[i64], task: Option<&TaskId>, at: i64) -> PortResult<()> {
        let mut notes = self.notes.lock().unwrap();
        for id in ids {
            if let Some(n) = notes.iter_mut().find(|n| n.id == *id) {
                n.uses += 1;
                n.last_used = Some(at);
            }
            self.uses.lock().unwrap().push((*id, task.cloned(), at));
        }
        Ok(())
    }
    fn notes_used_by(&self, task: &TaskId) -> PortResult<Vec<Note>> {
        let used: Vec<i64> = self
            .uses
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, t, _)| t.as_ref() == Some(task))
            .map(|(id, _, _)| *id)
            .collect();
        Ok(self
            .notes
            .lock()
            .unwrap()
            .iter()
            .filter(|n| used.contains(&n.id))
            .cloned()
            .collect())
    }
    fn note_use_stats(&self, project: &Path) -> PortResult<Vec<(i64, NoteUse)>> {
        let in_project = self.project_tasks(project);
        let mut seen: HashMap<i64, (HashSet<TaskId>, i64)> = HashMap::new();
        for (id, task, at) in self.uses.lock().unwrap().iter() {
            let Some(task) = task.as_ref().filter(|t| in_project.contains(t)) else {
                continue;
            };
            let entry = seen.entry(*id).or_default();
            entry.0.insert(task.clone());
            entry.1 = entry.1.max(*at);
        }
        Ok(seen
            .into_iter()
            .map(|(id, (tasks, last_used))| {
                let tasks = tasks.len() as u32;
                (id, NoteUse { tasks, last_used })
            })
            .collect())
    }
    fn dismiss_pair(&self, a: i64, b: i64) -> PortResult<()> {
        self.dismissed.lock().unwrap().push((a.min(b), a.max(b)));
        Ok(())
    }
    fn dismissed_pairs(&self) -> PortResult<Vec<(i64, i64)>> {
        Ok(self.dismissed.lock().unwrap().clone())
    }
}

impl SkillRepository for MemoryStore {
    fn add_skill(&self, s: &NewSkill) -> PortResult<Skill> {
        let mut skills = self.skills.lock().unwrap();
        let skill = Skill {
            id: skills.iter().map(|s| s.id).max().unwrap_or(0) + 1,
            name: s.name.clone(),
            description: s.description.clone(),
            body: s.body.clone(),
            project: s.project.clone(),
            status: s.status,
            author: s.author.clone(),
            created_at: s.created_at,
            updated_at: None,
            source_task: s.source_task.clone(),
        };
        skills.push(skill.clone());
        Ok(skill)
    }
    fn skill(&self, id: i64) -> PortResult<Option<Skill>> {
        Ok(self
            .skills
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == id)
            .cloned())
    }
    fn revise_skill(&self, id: i64, r: &SkillRevision) -> PortResult<()> {
        if let Some(s) = self.skills.lock().unwrap().iter_mut().find(|s| s.id == id) {
            s.project = r.project.clone();
            s.name = r.name.clone();
            s.description = r.description.clone();
            s.body = r.body.clone();
            s.author = r.author.clone();
            s.updated_at = Some(r.updated_at);
        }
        Ok(())
    }
    fn set_skill_status(&self, id: i64, status: SkillStatus) -> PortResult<()> {
        if let Some(s) = self.skills.lock().unwrap().iter_mut().find(|s| s.id == id) {
            s.status = status;
        }
        Ok(())
    }
    fn delete_skill(&self, id: i64) -> PortResult<()> {
        self.skills.lock().unwrap().retain(|s| s.id != id);
        Ok(())
    }
    fn skills(&self, project: Option<&Path>, with_global: bool) -> PortResult<Vec<Skill>> {
        let mut skills: Vec<Skill> = self
            .skills
            .lock()
            .unwrap()
            .iter()
            .filter(|s| match (&s.project, project) {
                (None, Some(_)) => with_global,
                (None, None) => true,
                (Some(p), Some(q)) => p == q,
                (Some(_), None) => false,
            })
            .cloned()
            .collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        Ok(skills)
    }
}

impl TaskRepository for MemoryStore {
    fn save_task(&self, task: &Task) -> PortResult<()> {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.retain(|t| t.id != task.id);
        tasks.push(task.clone());
        Ok(())
    }
    fn task(&self, id: &TaskId) -> PortResult<Option<Task>> {
        Ok(self
            .tasks
            .lock()
            .unwrap()
            .iter()
            .find(|t| &t.id == id)
            .cloned())
    }
    fn tasks(&self, project: &Path) -> PortResult<Vec<Task>> {
        let mut tasks: Vec<_> = self
            .tasks
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.project == project)
            .cloned()
            .collect();
        tasks.sort_by_key(|t| std::cmp::Reverse(t.created_at));
        Ok(tasks)
    }
    fn recent_tasks(&self, since: i64) -> PortResult<Vec<Task>> {
        let events = self.events.lock().unwrap();
        let mut tasks: Vec<_> = self
            .tasks
            .lock()
            .unwrap()
            .iter()
            .filter(|t| {
                matches!(t.status, TaskStatus::Running | TaskStatus::WaitingApproval)
                    || t.created_at >= since
                    || events
                        .get(&t.id)
                        .is_some_and(|e| e.iter().any(|(at, _)| *at >= since))
            })
            .cloned()
            .collect();
        tasks.sort_by_key(|t| std::cmp::Reverse(t.created_at));
        Ok(tasks)
    }
    fn append_event(&self, id: &TaskId, at: i64, event: &AgentEvent) -> PortResult<()> {
        self.events
            .lock()
            .unwrap()
            .entry(id.clone())
            .or_default()
            .push((at, event.clone()));
        Ok(())
    }
    fn events(&self, id: &TaskId) -> PortResult<Vec<AgentEvent>> {
        Ok(self.timed_events(id)?.into_iter().map(|(_, e)| e).collect())
    }
    fn timed_events(&self, id: &TaskId) -> PortResult<Vec<(i64, AgentEvent)>> {
        Ok(self
            .events
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .unwrap_or_default())
    }
    fn waiting_tasks(&self) -> PortResult<Vec<Task>> {
        Ok(self
            .tasks
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.wait.is_some())
            .cloned()
            .collect())
    }
    fn conversations(&self) -> PortResult<Vec<Task>> {
        let mut tasks: Vec<_> = self
            .tasks
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.conversation)
            .cloned()
            .collect();
        tasks.sort_by_key(|t| std::cmp::Reverse(t.created_at));
        Ok(tasks)
    }
    fn continuations(&self, conversation: &TaskId) -> PortResult<Vec<Task>> {
        let mut tasks: Vec<_> = self
            .tasks
            .lock()
            .unwrap()
            .iter()
            .filter(|t| {
                t.origin
                    .as_ref()
                    .is_some_and(|o| &o.conversation == conversation)
            })
            .cloned()
            .collect();
        tasks.sort_by_key(|t| std::cmp::Reverse(t.created_at));
        Ok(tasks)
    }
    fn delete_task(&self, id: &TaskId) -> PortResult<()> {
        self.tasks.lock().unwrap().retain(|t| &t.id != id);
        self.events.lock().unwrap().remove(id);
        self.queue.lock().unwrap().retain(|m| &m.task != id);
        Ok(())
    }
}

impl UsageRepository for MemoryStore {
    fn record_usage(&self, record: &UsageRecord) -> PortResult<()> {
        self.usage.lock().unwrap().push(record.clone());
        Ok(())
    }
    fn usage_since(&self, account: &AccountId, since: i64) -> PortResult<UsageSample> {
        let mut total = UsageSample::default();
        for r in self.usage.lock().unwrap().iter() {
            if &r.account == account && r.at >= since {
                total.add(&r.sample);
            }
        }
        Ok(total)
    }
    fn usage_by_project(&self, since: i64) -> PortResult<Vec<(PathBuf, AccountId, UsageSample)>> {
        let mut totals: BTreeMap<(PathBuf, AccountId), UsageSample> = BTreeMap::new();
        for r in self.usage.lock().unwrap().iter().filter(|r| r.at >= since) {
            totals
                .entry((r.project.clone(), r.account.clone()))
                .or_default()
                .add(&r.sample);
        }
        Ok(totals
            .into_iter()
            .map(|((project, account), usage)| (project, account, usage))
            .collect())
    }
    fn usage_points(&self, project: &Path, since: i64) -> PortResult<Vec<(i64, UsageSample)>> {
        Ok(self
            .usage
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.project == project && r.at >= since)
            .map(|r| (r.at, r.sample.clone()))
            .collect())
    }
    fn set_rate_limit(&self, account: &AccountId, limit: &RateLimit) -> PortResult<()> {
        self.limits
            .lock()
            .unwrap()
            .insert(account.clone(), limit.clone());
        Ok(())
    }
    fn rate_limit(&self, account: &AccountId) -> PortResult<Option<RateLimit>> {
        Ok(self.limits.lock().unwrap().get(account).cloned())
    }
    fn set_plan_usage(&self, account: &AccountId, record: &PlanUsageRecord) -> PortResult<()> {
        self.plan_usage
            .lock()
            .unwrap()
            .insert(account.clone(), record.clone());
        Ok(())
    }
    fn plan_usage(&self, account: &AccountId) -> PortResult<Option<PlanUsageRecord>> {
        Ok(self.plan_usage.lock().unwrap().get(account).cloned())
    }
}

/// Every call ticks one second, so task ids are unique.
pub struct FakeClock(AtomicI64);

impl FakeClock {
    pub fn advance(&self, secs: i64) {
        self.0.fetch_add(secs * 1000, Ordering::Relaxed);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0.fetch_add(1000, Ordering::Relaxed)
    }
    /// No time-zone database: any zoned time is "an hour after `after`".
    fn resolve_wall_time(
        &self,
        _wall: &crate::domain::plan_usage::WallTime,
        after: i64,
    ) -> Option<i64> {
        Some(after + 3600)
    }
}

/// Answers `account/rateLimits/read` with a canned result.
#[derive(Default)]
pub struct FakeUsageProbe {
    reply: Mutex<Option<PortResult<String>>>,
    calls: Mutex<Vec<LaunchSpec>>,
    balance: Mutex<Option<PortResult<String>>>,
    balance_calls: Mutex<Vec<(String, String)>>,
}

impl FakeUsageProbe {
    pub fn reply(&self, result: PortResult<String>) {
        *self.reply.lock().unwrap() = Some(result);
    }
    pub fn calls(&self) -> Vec<LaunchSpec> {
        self.calls.lock().unwrap().clone()
    }
    pub fn reply_balance(&self, result: PortResult<String>) {
        *self.balance.lock().unwrap() = Some(result);
    }
    /// (base URL, key) of each balance request.
    pub fn balance_calls(&self) -> Vec<(String, String)> {
        self.balance_calls.lock().unwrap().clone()
    }
}

impl UsageProbe for FakeUsageProbe {
    fn codex_rate_limits(
        &self,
        spec: &LaunchSpec,
        _timeout: std::time::Duration,
    ) -> PortResult<String> {
        self.calls.lock().unwrap().push(spec.clone());
        self.reply
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Err(PortError::new("no reply configured")))
    }
    fn moonshot_balance(
        &self,
        base_url: &str,
        api_key: &super::ports::Secret,
        _timeout: std::time::Duration,
    ) -> PortResult<String> {
        self.balance_calls
            .lock()
            .unwrap()
            .push((base_url.to_owned(), api_key.expose().to_owned()));
        self.balance
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Err(PortError::new("no reply configured")))
    }
}

/// Records git operations instead of running them.
pub struct FakeGit {
    log: Mutex<Vec<String>>,
    excluded: Mutex<Vec<String>>,
    ignored: Mutex<Vec<String>>,
    merge: Mutex<MergeOutcome>,
    /// The main checkout's branch and HEAD; a merge moves HEAD to `merged`.
    branch: Mutex<String>,
    head: Mutex<String>,
    remotes: Mutex<Vec<String>>,
    reset_error: Mutex<Option<String>>,
    diff: Mutex<Vec<FileDiff>>,
    worktrees: Mutex<Vec<String>>,
    init_error: Mutex<Option<String>>,
    files: Mutex<Vec<String>>,
    staged: Mutex<Vec<FileDiff>>,
    commits: Mutex<Vec<Commit>>,
}

impl Default for FakeGit {
    fn default() -> Self {
        Self {
            log: Mutex::default(),
            excluded: Mutex::default(),
            ignored: Mutex::default(),
            merge: Mutex::new(MergeOutcome::Merged),
            branch: Mutex::new("main".into()),
            head: Mutex::new("abc123".into()),
            remotes: Mutex::default(),
            reset_error: Mutex::default(),
            diff: Mutex::default(),
            worktrees: Mutex::default(),
            init_error: Mutex::default(),
            files: Mutex::default(),
            staged: Mutex::default(),
            commits: Mutex::default(),
        }
    }
}

impl FakeGit {
    /// What `list_files` answers.
    pub fn set_files(&self, files: &[&str]) {
        *self.files.lock().unwrap() = files.iter().map(|f| f.to_string()).collect();
    }
    pub fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
    pub fn excluded(&self) -> Vec<String> {
        self.excluded.lock().unwrap().clone()
    }
    pub fn ignore(&self, path: &str) {
        self.ignored.lock().unwrap().push(path.into());
    }
    pub fn set_merge_outcome(&self, outcome: MergeOutcome) {
        *self.merge.lock().unwrap() = outcome;
    }
    /// The user switches branch in the main checkout.
    pub fn set_branch(&self, branch: &str) {
        *self.branch.lock().unwrap() = branch.into();
    }
    /// A new commit lands on the main checkout's branch.
    pub fn set_head(&self, commit: &str) {
        *self.head.lock().unwrap() = commit.into();
    }
    pub fn head(&self) -> String {
        self.head.lock().unwrap().clone()
    }
    /// Remote branches that contain any commit (it was pushed).
    pub fn set_remotes(&self, remotes: &[&str]) {
        *self.remotes.lock().unwrap() = remotes.iter().map(|r| r.to_string()).collect();
    }
    /// The next `reset_keep` fails with git's message.
    pub fn fail_reset(&self, message: &str) {
        *self.reset_error.lock().unwrap() = Some(message.into());
    }
    pub fn set_diff(&self, diff: Vec<FileDiff>) {
        *self.diff.lock().unwrap() = diff;
    }
    /// The next `init_repo` fails with git's message.
    pub fn fail_init(&self, message: &str) {
        *self.init_error.lock().unwrap() = Some(message.into());
    }
    /// What `staged_diff` answers.
    pub fn set_staged(&self, diff: Vec<FileDiff>) {
        *self.staged.lock().unwrap() = diff;
    }
    /// What `recent_commits` answers (subjects, newest first).
    pub fn set_commits(&self, subjects: &[&str]) {
        *self.commits.lock().unwrap() = subjects
            .iter()
            .enumerate()
            .map(|(i, s)| Commit {
                hash: format!("c{i}"),
                subject: s.to_string(),
            })
            .collect();
    }
    /// Worktrees added: "<branch> at <commit>".
    pub fn worktrees(&self) -> Vec<String> {
        self.worktrees.lock().unwrap().clone()
    }
}

impl Git for FakeGit {
    fn uncommitted_changes(&self, _: &Path) -> PortResult<usize> {
        Ok(3)
    }
    fn current_branch(&self, _: &Path) -> PortResult<String> {
        Ok(self.branch.lock().unwrap().clone())
    }
    fn head_commit(&self, _: &Path) -> PortResult<String> {
        Ok(self.head())
    }
    fn ensure_excluded(&self, _: &Path, pattern: &str) -> PortResult<()> {
        self.excluded.lock().unwrap().push(pattern.into());
        Ok(())
    }
    fn add_worktree(&self, _: &Path, _: &Path, branch: &str, base: &str) -> PortResult<()> {
        self.worktrees
            .lock()
            .unwrap()
            .push(format!("{branch} at {base}"));
        Ok(())
    }
    fn is_ignored(&self, _: &Path, path: &str) -> bool {
        self.ignored.lock().unwrap().iter().any(|p| p == path)
    }
    fn diff(&self, _: &Path, _: &str) -> PortResult<Vec<FileDiff>> {
        Ok(self.diff.lock().unwrap().clone())
    }
    fn commit_all(&self, worktree: &Path, message: &str) -> PortResult<bool> {
        self.log
            .lock()
            .unwrap()
            .push(format!("commit {} {message}", worktree.display()));
        Ok(true)
    }
    fn merge(&self, _: &Path, base: &str, branch: &str) -> PortResult<MergeOutcome> {
        self.log
            .lock()
            .unwrap()
            .push(format!("merge {base} <- {branch}"));
        let outcome = self.merge.lock().unwrap().clone();
        if outcome == MergeOutcome::Merged {
            self.set_head("merged");
        }
        Ok(outcome)
    }
    fn remove_worktree(&self, _: &Path, _: &Path, branch: &str) -> PortResult<()> {
        self.log.lock().unwrap().push(format!("remove {branch}"));
        Ok(())
    }
    fn recent_commits(&self, _: &Path, limit: usize) -> PortResult<Vec<Commit>> {
        let commits = self.commits.lock().unwrap();
        Ok(commits.iter().take(limit).cloned().collect())
    }
    fn staged_diff(&self, _: &Path) -> PortResult<Vec<FileDiff>> {
        Ok(self.staged.lock().unwrap().clone())
    }
    fn commit_staged(&self, repo: &Path, message: &str, edit: bool) -> PortResult<()> {
        let edit = if edit { " (edit)" } else { "" };
        self.log
            .lock()
            .unwrap()
            .push(format!("commit-staged {}{edit} {message}", repo.display()));
        Ok(())
    }
    fn rev_parse(&self, _: &Path, rev: &str) -> PortResult<String> {
        Ok(if rev == "HEAD" {
            "tip".into()
        } else {
            rev.into()
        })
    }
    fn remote_branches_containing(&self, _: &Path, _: &str) -> PortResult<Vec<String>> {
        Ok(self.remotes.lock().unwrap().clone())
    }
    fn list_files(&self, _repo: &Path) -> PortResult<Vec<String>> {
        Ok(self.files.lock().unwrap().clone())
    }
    fn init_repo(&self, dir: &Path, message: &str) -> PortResult<()> {
        if let Some(error) = self.init_error.lock().unwrap().take() {
            return Err(PortError::new(error));
        }
        self.log
            .lock()
            .unwrap()
            .push(format!("init {} {message}", dir.display()));
        Ok(())
    }
    fn reset_keep(&self, _: &Path, commit: &str) -> PortResult<()> {
        if let Some(message) = self.reset_error.lock().unwrap().take() {
            return Err(PortError::new(message));
        }
        self.log
            .lock()
            .unwrap()
            .push(format!("reset --keep {commit}"));
        self.set_head(commit);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentCall {
    Send(String),
    Answer(String, bool),
    Stop,
}

/// Agent double: the test plays the agent's side with `emit`.
#[derive(Default)]
pub struct FakeAgents {
    requests: Mutex<Vec<AgentRequest>>,
    sinks: Mutex<Vec<EventSink>>,
    calls: Arc<Mutex<Vec<AgentCall>>>,
    messages: Arc<Mutex<Vec<Option<String>>>>,
    fail_next: Mutex<bool>,
    /// Images sent with each follow-up, in order.
    sent_images: Arc<Mutex<Vec<Vec<PromptImage>>>>,
    /// Overrides what every backend says about images.
    image_support: Mutex<Option<ImageSupport>>,
    /// Events the next agent plays as soon as it starts.
    script: Mutex<Option<Vec<AgentEvent>>>,
}

impl FakeAgents {
    /// The next agent started plays `events` right away (for use cases that
    /// wait for an answer).
    pub fn reply_on_start(&self, events: Vec<AgentEvent>) {
        *self.script.lock().unwrap() = Some(events);
    }
    /// Every backend answers `support` about images.
    pub fn set_image_support(&self, support: ImageSupport) {
        *self.image_support.lock().unwrap() = Some(support);
    }
    /// Images sent with each follow-up, in order.
    pub fn sent_images(&self) -> Vec<Vec<PromptImage>> {
        self.sent_images.lock().unwrap().clone()
    }
    /// The next `start` fails, as when the CLI is missing.
    pub fn fail_next_start(&self) {
        *self.fail_next.lock().unwrap() = true;
    }

    pub fn last_request(&self) -> AgentRequest {
        self.requests
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("no agent started")
    }
    pub fn calls(&self) -> Vec<AgentCall> {
        self.calls.lock().unwrap().clone()
    }
    /// Messages sent with each answer, in order.
    pub fn answer_messages(&self) -> Vec<Option<String>> {
        self.messages.lock().unwrap().clone()
    }
    pub fn requests(&self) -> Vec<AgentRequest> {
        self.requests.lock().unwrap().clone()
    }
    /// Plays the most recently started agent.
    pub fn emit(&self, event: AgentEvent) {
        let sink = self
            .sinks
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("no agent started");
        sink(event);
    }
    /// Plays the `n`-th agent started (0-based), even after it was replaced.
    pub fn emit_from(&self, n: usize, event: AgentEvent) {
        let sink = self.sinks.lock().unwrap()[n].clone();
        sink(event);
    }
}

struct FakeSession {
    calls: Arc<Mutex<Vec<AgentCall>>>,
    messages: Arc<Mutex<Vec<Option<String>>>>,
    sent_images: Arc<Mutex<Vec<Vec<PromptImage>>>>,
}

impl AgentSession for FakeSession {
    fn send(&mut self, text: &str, images: &[PromptImage]) -> PortResult<()> {
        self.calls
            .lock()
            .unwrap()
            .push(AgentCall::Send(text.into()));
        self.sent_images.lock().unwrap().push(images.to_vec());
        Ok(())
    }
    fn answer(&mut self, request_id: &str, allow: bool, message: Option<&str>) -> PortResult<()> {
        self.calls
            .lock()
            .unwrap()
            .push(AgentCall::Answer(request_id.into(), allow));
        self.messages
            .lock()
            .unwrap()
            .push(message.map(str::to_owned));
        Ok(())
    }
    fn stop(&mut self) {
        self.calls.lock().unwrap().push(AgentCall::Stop);
    }
}

impl AgentLauncher for FakeAgents {
    fn start(&self, request: AgentRequest, events: EventSink) -> PortResult<Box<dyn AgentSession>> {
        if std::mem::take(&mut *self.fail_next.lock().unwrap()) {
            return Err(PortError::new("no se encontró el programa"));
        }
        self.requests.lock().unwrap().push(request);
        if let Some(script) = self.script.lock().unwrap().take() {
            for event in script {
                events(event);
            }
        }
        self.sinks.lock().unwrap().push(events);
        Ok(Box::new(FakeSession {
            calls: Arc::clone(&self.calls),
            messages: Arc::clone(&self.messages),
            sent_images: Arc::clone(&self.sent_images),
        }))
    }

    /// Like the real router, unless a test says otherwise.
    fn image_support(&self, backend: &AgentBackend) -> ImageSupport {
        if let Some(support) = *self.image_support.lock().unwrap() {
            return support;
        }
        match backend {
            AgentBackend::Acp { .. } => ImageSupport::IfAgentAllows,
            _ => ImageSupport::Yes,
        }
    }
}

impl AccountRepository for MemoryStore {
    fn list_accounts(&self) -> PortResult<Vec<Account>> {
        Ok(self.accounts.lock().unwrap().clone())
    }
    fn save_account(&self, account: &Account) -> PortResult<()> {
        let mut accounts = self.accounts.lock().unwrap();
        accounts.retain(|a| a.id != account.id);
        accounts.push(account.clone());
        Ok(())
    }
    fn delete_account(&self, id: &AccountId) -> PortResult<()> {
        self.accounts.lock().unwrap().retain(|a| &a.id != id);
        Ok(())
    }
}

impl ProjectRepository for MemoryStore {
    fn list_projects(&self) -> PortResult<Vec<Project>> {
        Ok(self.projects.lock().unwrap().clone())
    }
    fn save_project(&self, project: &Project) -> PortResult<()> {
        let mut projects = self.projects.lock().unwrap();
        match projects.iter_mut().find(|p| p.path == project.path) {
            Some(p) => *p = project.clone(),
            None => projects.push(project.clone()),
        }
        Ok(())
    }
    fn delete_project(&self, path: &Path) -> PortResult<()> {
        self.projects.lock().unwrap().retain(|p| p.path != path);
        Ok(())
    }
}

impl SettingsRepository for MemoryStore {
    fn get_setting(&self, key: &str) -> PortResult<Option<String>> {
        Ok(self.settings.lock().unwrap().get(key).cloned())
    }
    fn set_setting(&self, key: &str, value: Option<&str>) -> PortResult<()> {
        let mut settings = self.settings.lock().unwrap();
        match value {
            Some(v) => settings.insert(key.to_owned(), v.to_owned()),
            None => settings.remove(key),
        };
        Ok(())
    }
}

#[derive(Default)]
pub struct MemorySecrets(Mutex<BTreeMap<String, String>>);

impl MemorySecrets {
    pub fn get(&self, account: &str) -> Option<String> {
        self.0.lock().unwrap().get(account).cloned()
    }
}

impl SecretStore for MemorySecrets {
    fn set_api_key(&self, account: &AccountId, key: &str) -> PortResult<()> {
        self.0
            .lock()
            .unwrap()
            .insert(account.to_string(), key.to_owned());
        Ok(())
    }
    fn api_key(&self, account: &AccountId) -> PortResult<Option<String>> {
        Ok(self.0.lock().unwrap().get(account.as_str()).cloned())
    }
    fn has_api_key(&self, account: &AccountId) -> PortResult<bool> {
        Ok(self.0.lock().unwrap().contains_key(account.as_str()))
    }
    fn delete_api_key(&self, account: &AccountId) -> PortResult<()> {
        self.0.lock().unwrap().remove(account.as_str());
        Ok(())
    }
    fn sync_keys(&self) -> PortResult<Option<String>> {
        Ok(self.0.lock().unwrap().get(SYNC_KEYS).cloned())
    }
    fn set_sync_keys(&self, keys: &str) -> PortResult<()> {
        self.0
            .lock()
            .unwrap()
            .insert(SYNC_KEYS.into(), keys.to_owned());
        Ok(())
    }
    fn delete_sync_keys(&self) -> PortResult<()> {
        self.0.lock().unwrap().remove(SYNC_KEYS);
        Ok(())
    }
}

/// Where [`MemorySecrets`] keeps the sync keys (not an account id: those
/// never contain a space).
const SYNC_KEYS: &str = "sync keys";

/// Records launches and answers captured runs with a canned reply, or with
/// the reply set for that exact command line.
#[derive(Default)]
pub struct FakeRunner {
    interactive: Mutex<Vec<LaunchSpec>>,
    detached: Mutex<Vec<LaunchSpec>>,
    captured: Mutex<Vec<LaunchSpec>>,
    reply: Mutex<Option<CapturedOutput>>,
    replies: Mutex<HashMap<String, PortResult<CapturedOutput>>>,
    installed: Mutex<HashSet<String>>,
    env: Mutex<Vec<String>>,
}

fn command_line(spec: &LaunchSpec) -> String {
    std::iter::once(spec.program.as_str())
        .chain(spec.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

impl FakeRunner {
    pub fn install(&self, program: &str) {
        self.installed.lock().unwrap().insert(program.to_owned());
    }

    pub fn reply(&self, stdout: &str, exit_code: i32) {
        *self.reply.lock().unwrap() = Some(CapturedOutput {
            exit_code: Some(exit_code),
            stdout: stdout.to_owned(),
        });
    }
    /// The reply to exactly `command` (`"claude --version"`).
    pub fn reply_to(&self, command: &str, stdout: &str, exit_code: i32) {
        self.replies.lock().unwrap().insert(
            command.into(),
            Ok(CapturedOutput {
                exit_code: Some(exit_code),
                stdout: stdout.to_owned(),
            }),
        );
    }
    /// `command` fails, as when it times out.
    pub fn fail(&self, command: &str) {
        self.replies
            .lock()
            .unwrap()
            .insert(command.into(), Err(PortError::new("timed out")));
    }
    /// Variables in Hedy's own environment that CLIs must not inherit.
    pub fn set_env(&self, names: &[&str]) {
        *self.env.lock().unwrap() = names.iter().map(|n| n.to_string()).collect();
    }
    pub fn captured(&self) -> Vec<LaunchSpec> {
        self.captured.lock().unwrap().clone()
    }
    pub fn interactive(&self) -> Vec<LaunchSpec> {
        self.interactive.lock().unwrap().clone()
    }
    pub fn detached(&self) -> Vec<LaunchSpec> {
        self.detached.lock().unwrap().clone()
    }
}

impl ProcessRunner for FakeRunner {
    fn run_interactive(&self, spec: &LaunchSpec) -> PortResult<Option<i32>> {
        self.interactive.lock().unwrap().push(spec.clone());
        Ok(Some(0))
    }
    fn run_captured(&self, spec: &LaunchSpec) -> PortResult<CapturedOutput> {
        self.captured.lock().unwrap().push(spec.clone());
        if let Some(reply) = self.replies.lock().unwrap().get(&command_line(spec)) {
            return reply.clone();
        }
        self.reply
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| PortError::new("no reply configured"))
    }
    fn run_captured_within(
        &self,
        spec: &LaunchSpec,
        _timeout: std::time::Duration,
    ) -> PortResult<CapturedOutput> {
        self.run_captured(spec)
    }
    fn blocked_env_vars(&self) -> Vec<String> {
        self.env.lock().unwrap().clone()
    }
    fn spawn_detached(&self, spec: &LaunchSpec) -> PortResult<()> {
        self.detached.lock().unwrap().push(spec.clone());
        Ok(())
    }
    fn is_installed(&self, program: &str) -> bool {
        self.installed.lock().unwrap().contains(program)
    }
    fn exec(&self, _spec: &LaunchSpec) -> PortError {
        PortError::new("exec is not available in tests")
    }
}

/// Paths are taken as already canonical; account folders live under `/data`.
#[derive(Default)]
pub struct FakeFs {
    existing: Mutex<HashSet<PathBuf>>,
    links: Mutex<Vec<(PathBuf, PathBuf)>>,
    dirs: Mutex<HashSet<PathBuf>>,
    files: Mutex<Vec<(PathBuf, String)>>,
    /// Binary files (attachments).
    blobs: Mutex<Vec<(PathBuf, Vec<u8>)>>,
    git_roots: Mutex<Vec<PathBuf>>,
    read_only: Mutex<HashSet<PathBuf>>,
    removed_dirs: Mutex<Vec<PathBuf>>,
    /// Folders created by `create_new_dir`, then gone with `remove_dir_all`.
    created: Mutex<Vec<PathBuf>>,
    deleted_trees: Mutex<Vec<PathBuf>>,
}

impl FakeFs {
    pub fn dirs(&self) -> HashSet<PathBuf> {
        self.dirs.lock().unwrap().clone()
    }
    pub fn files(&self) -> Vec<(PathBuf, String)> {
        self.files.lock().unwrap().clone()
    }
    pub fn add_existing(&self, path: &str) {
        self.existing.lock().unwrap().insert(path.into());
    }
    pub fn symlinks(&self) -> Vec<(PathBuf, PathBuf)> {
        self.links.lock().unwrap().clone()
    }
    pub fn make_read_only(&self, path: &str) {
        self.read_only.lock().unwrap().insert(path.into());
    }
    /// The text of the file at `path`, if any.
    pub fn file(&self, path: &str) -> Option<String> {
        self.read_text(Path::new(path)).unwrap()
    }
    pub fn add_file(&self, path: &str, contents: &str) {
        self.write_text(Path::new(path), contents).unwrap();
    }
    pub fn removed_dirs(&self) -> Vec<PathBuf> {
        self.removed_dirs.lock().unwrap().clone()
    }
    pub fn add_git_root(&self, path: &str) {
        self.git_roots.lock().unwrap().push(path.into());
    }
    pub fn created(&self) -> Vec<PathBuf> {
        self.created.lock().unwrap().clone()
    }
    pub fn deleted_trees(&self) -> Vec<PathBuf> {
        self.deleted_trees.lock().unwrap().clone()
    }
    /// Paths of the binary files written (attachments).
    pub fn blob_paths(&self) -> Vec<PathBuf> {
        self.blobs
            .lock()
            .unwrap()
            .iter()
            .map(|(p, _)| p.clone())
            .collect()
    }
    /// Removes a binary file, as when the user deletes it by hand.
    pub fn delete_blob(&self, path: &str) {
        self.blobs
            .lock()
            .unwrap()
            .retain(|(p, _)| p != Path::new(path));
    }
}

impl FileSystem for FakeFs {
    fn account_folder(&self, id: &AccountId) -> PathBuf {
        Path::new("/data/accounts").join(id.as_str())
    }
    fn chats_dir(&self) -> PathBuf {
        PathBuf::from("/data/chats")
    }
    fn attachments_dir(&self) -> PathBuf {
        PathBuf::from("/data/attachments")
    }
    fn home_dir(&self) -> Option<PathBuf> {
        Some(PathBuf::from("/home/e"))
    }
    fn create_new_dir(&self, path: &Path) -> PortResult<()> {
        if self.exists(path) {
            return Err(PortError::new("File exists"));
        }
        if let Some(parent) = path.parent()
            && self.read_only.lock().unwrap().contains(parent)
        {
            return Err(PortError::new("permission denied"));
        }
        self.existing.lock().unwrap().insert(path.to_owned());
        self.created.lock().unwrap().push(path.to_owned());
        Ok(())
    }
    fn remove_dir_all(&self, path: &Path) -> PortResult<()> {
        self.existing.lock().unwrap().remove(path);
        self.dirs.lock().unwrap().remove(path);
        self.deleted_trees.lock().unwrap().push(path.to_owned());
        Ok(())
    }
    fn create_private_dir(&self, path: &Path) -> PortResult<()> {
        self.dirs.lock().unwrap().insert(path.to_owned());
        Ok(())
    }
    fn exists(&self, path: &Path) -> bool {
        self.existing.lock().unwrap().contains(path)
            || self.files.lock().unwrap().iter().any(|(p, _)| p == path)
    }
    fn check_writable(&self, dir: &Path) -> PortResult<()> {
        if self.read_only.lock().unwrap().contains(dir) {
            Err(PortError::new("permission denied"))
        } else {
            Ok(())
        }
    }
    fn symlink_dir(&self, target: &Path, link: &Path) -> PortResult<()> {
        self.links
            .lock()
            .unwrap()
            .push((target.to_owned(), link.to_owned()));
        Ok(())
    }
    fn write_if_missing(&self, path: &Path, contents: &str) -> PortResult<()> {
        let mut files = self.files.lock().unwrap();
        if !files.iter().any(|(p, _)| p == path) {
            files.push((path.to_owned(), contents.to_owned()));
        }
        Ok(())
    }
    fn read_text(&self, path: &Path) -> PortResult<Option<String>> {
        Ok(self
            .files
            .lock()
            .unwrap()
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, text)| text.clone()))
    }
    fn write_text(&self, path: &Path, contents: &str) -> PortResult<()> {
        if self.read_only.lock().unwrap().contains(path) {
            return Err(PortError::new("permission denied"));
        }
        let mut files = self.files.lock().unwrap();
        files.retain(|(p, _)| p != path);
        files.push((path.to_owned(), contents.to_owned()));
        Ok(())
    }
    fn write_private_file(&self, path: &Path, contents: &[u8]) -> PortResult<()> {
        let mut blobs = self.blobs.lock().unwrap();
        if blobs.iter().any(|(p, _)| p == path) {
            return Err(PortError::new("File exists"));
        }
        if let Some(dir) = path.parent() {
            self.dirs.lock().unwrap().insert(dir.to_owned());
        }
        blobs.push((path.to_owned(), contents.to_owned()));
        Ok(())
    }
    fn read_bytes(&self, path: &Path) -> PortResult<Option<Vec<u8>>> {
        Ok(self
            .blobs
            .lock()
            .unwrap()
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, bytes)| bytes.clone()))
    }
    fn remove_file(&self, path: &Path) -> PortResult<()> {
        self.files.lock().unwrap().retain(|(p, _)| p != path);
        Ok(())
    }
    fn remove_empty_dir(&self, path: &Path) -> PortResult<()> {
        self.removed_dirs.lock().unwrap().push(path.to_owned());
        Ok(())
    }
    fn canonicalize(&self, dir: &Path) -> PortResult<PathBuf> {
        Ok(dir.to_owned())
    }
    fn git_root(&self, dir: &Path) -> Option<PathBuf> {
        self.git_roots
            .lock()
            .unwrap()
            .iter()
            .find(|root| dir.starts_with(root))
            .cloned()
    }
}

/// Records the notices the use cases send to the driving adapter.
#[derive(Default)]
pub struct RecordedNotices(
    Mutex<Vec<(TaskId, Notice)>>,
    Mutex<Vec<usize>>,
    Mutex<Vec<Task>>,
);

impl RecordedNotices {
    /// The length of the queue after each change the UI heard about.
    pub fn queue_lengths(&self) -> Vec<usize> {
        self.1.lock().unwrap().clone()
    }

    pub fn kinds(&self) -> Vec<NoticeKind> {
        self.0.lock().unwrap().iter().map(|(_, n)| n.kind).collect()
    }

    /// Every `task_updated` the UI heard, in order.
    pub fn updated(&self) -> Vec<Task> {
        self.2.lock().unwrap().clone()
    }

    pub fn titles(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|(_, n)| n.title.clone())
            .collect()
    }
}

impl TaskEvents for RecordedNotices {
    fn event(&self, _: &TaskId, _: &AgentEvent) {}
    fn task_updated(&self, task: &Task) {
        self.2.lock().unwrap().push(task.clone());
    }
    fn notice(&self, task: &Task, notice: &Notice) {
        self.0
            .lock()
            .unwrap()
            .push((task.id.clone(), notice.clone()));
    }
    fn queue_changed(&self, _: &TaskId, queue: &[QueuedMessage]) {
        self.1.lock().unwrap().push(queue.len());
    }
}

pub struct Fixture {
    pub hedy: Hedy,
    pub notices: Arc<RecordedNotices>,
    pub store: Arc<MemoryStore>,
    pub secrets: Arc<MemorySecrets>,
    pub runner: Arc<FakeRunner>,
    pub fs: Arc<FakeFs>,
    pub git: Arc<FakeGit>,
    pub code: Arc<super::testing_code::FakeCode>,
    pub agents: Arc<FakeAgents>,
    pub clock: Arc<FakeClock>,
    pub probe: Arc<FakeUsageProbe>,
    pub source: Arc<FakeSource>,
    pub host: Arc<FakeHost>,
    pub themes: Arc<FakeThemes>,
    pub sync: Arc<FakeSyncRemote>,
}

/// Themes kept in memory, by id.
#[derive(Default)]
pub struct FakeThemes(Mutex<BTreeMap<String, CustomTheme>>);

impl ThemeStore for FakeThemes {
    fn list(&self) -> PortResult<Vec<CustomTheme>> {
        Ok(self.0.lock().unwrap().values().cloned().collect())
    }
    fn get(&self, id: &str) -> PortResult<Option<CustomTheme>> {
        Ok(self.0.lock().unwrap().get(id).cloned())
    }
    fn save(&self, theme: &CustomTheme) -> PortResult<()> {
        self.0
            .lock()
            .unwrap()
            .insert(theme.id.clone(), theme.clone());
        Ok(())
    }
    fn delete(&self, id: &str) -> PortResult<()> {
        self.0.lock().unwrap().remove(id);
        Ok(())
    }
    fn work_dir(&self) -> PortResult<PathBuf> {
        Ok(PathBuf::from("/data/themes"))
    }
}

impl Fixture {
    pub fn new() -> Self {
        Self::with_server(SyncServer::default())
    }

    /// A PC that syncs through `server`, shared with other fixtures (PCs).
    pub fn with_server(server: SyncServer) -> Self {
        let secrets = Arc::new(MemorySecrets::default());
        let runner = Arc::new(FakeRunner::default());
        let fs = Arc::new(FakeFs::default());
        let git = Arc::new(FakeGit::default());
        let code = Arc::new(super::testing_code::FakeCode::default());
        let agents = Arc::new(FakeAgents::default());
        let store = Arc::new(MemoryStore::default());
        let clock = Arc::new(FakeClock(AtomicI64::new(1_700_000_000_000)));
        let probe = Arc::new(FakeUsageProbe::default());
        let source = Arc::new(FakeSource::default());
        let host = Arc::new(FakeHost::default());
        let themes = Arc::new(FakeThemes::default());
        let sync = Arc::new(FakeSyncRemote::new(server));
        let hedy = Hedy::new(Ports {
            store: store.clone(),
            secrets: secrets.clone(),
            runner: runner.clone(),
            fs: fs.clone(),
            git: git.clone(),
            code: code.clone(),
            writer: code.clone(),
            agents: agents.clone(),
            clock: clock.clone(),
            usage: probe.clone(),
            source: source.clone(),
            host: host.clone(),
            themes: themes.clone(),
            sync_remote: sync.clone(),
            sync_crypto: Arc::new(crate::adapters::VaultCrypto),
        });
        let notices = Arc::new(RecordedNotices::default());
        let hedy = hedy.with_task_events(notices.clone());
        Self {
            hedy,
            notices,
            store,
            secrets,
            runner,
            fs,
            git,
            code,
            agents,
            clock,
            probe,
            source,
            host,
            themes,
            sync,
        }
    }

    /// This fixture's fakes around a real `store` and sync remote: what the
    /// sync tests run against SQLite and git.
    pub fn hedy_on(
        &self,
        store: Arc<dyn super::ports::Store>,
        sync: Arc<dyn super::ports::SyncRemote>,
    ) -> Hedy {
        Hedy::new(Ports {
            store,
            secrets: self.secrets.clone(),
            runner: self.runner.clone(),
            fs: self.fs.clone(),
            git: self.git.clone(),
            code: self.code.clone(),
            writer: self.code.clone(),
            agents: self.agents.clone(),
            clock: self.clock.clone(),
            usage: self.probe.clone(),
            source: self.source.clone(),
            host: self.host.clone(),
            themes: self.themes.clone(),
            sync_remote: sync,
            sync_crypto: Arc::new(crate::adapters::VaultCrypto),
        })
    }

    pub fn dir(&self, path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    pub fn add_claude(&self, name: &str) -> Account {
        self.add(name, Provider::Claude, false)
    }

    pub fn add_kimi(&self, name: &str) -> Account {
        self.add(name, Provider::Kimi, true)
    }

    pub fn add_codex(&self, name: &str) -> Account {
        self.add(name, Provider::Codex, false)
    }

    pub fn add(&self, name: &str, provider: Provider, api_key: bool) -> Account {
        self.hedy
            .add_account(NewAccount {
                name: name.into(),
                provider,
                api_key,
                default_folder: false,
                color: None,
            })
            .unwrap()
    }
}

impl QueueRepository for MemoryStore {
    fn add_queued(&self, task: &TaskId, text: &str, at: i64) -> PortResult<QueuedMessage> {
        let mut queue = self.queue.lock().unwrap();
        let message = QueuedMessage {
            id: queue.iter().map(|m| m.id).max().unwrap_or(0) + 1,
            task: task.clone(),
            text: text.into(),
            created_at: at,
            held: None,
        };
        queue.push(message.clone());
        Ok(message)
    }
    fn queued(&self, task: &TaskId) -> PortResult<Vec<QueuedMessage>> {
        Ok(self
            .queue
            .lock()
            .unwrap()
            .iter()
            .filter(|m| &m.task == task)
            .cloned()
            .collect())
    }
    fn queued_message(&self, id: i64) -> PortResult<Option<QueuedMessage>> {
        Ok(self
            .queue
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.id == id)
            .cloned())
    }
    fn update_queued(&self, id: i64, text: &str) -> PortResult<()> {
        if let Some(m) = self.queue.lock().unwrap().iter_mut().find(|m| m.id == id) {
            m.text = text.into();
        }
        Ok(())
    }
    fn delete_queued(&self, ids: &[i64]) -> PortResult<()> {
        self.queue.lock().unwrap().retain(|m| !ids.contains(&m.id));
        Ok(())
    }
    fn hold_queue(&self, task: &TaskId, reason: Option<HoldReason>) -> PortResult<()> {
        for m in self
            .queue
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|m| &m.task == task)
        {
            m.held = reason.and(m.held.or(reason));
        }
        Ok(())
    }
}

impl CommandRepository for MemoryStore {
    fn set_agent_commands(
        &self,
        account: &AccountId,
        project: Option<&Path>,
        known: &KnownCommands,
    ) -> PortResult<()> {
        let mut lists = self.commands.lock().unwrap();
        let project = project.map(Path::to_path_buf);
        lists.retain(|(a, p, _)| !(a == account && *p == project));
        lists.push((account.clone(), project, known.clone()));
        Ok(())
    }
    fn agent_commands(
        &self,
        account: &AccountId,
    ) -> PortResult<Vec<(Option<PathBuf>, KnownCommands)>> {
        let mut found: Vec<_> = self
            .commands
            .lock()
            .unwrap()
            .iter()
            .filter(|(a, _, _)| a == account)
            .map(|(_, p, k)| (p.clone(), k.clone()))
            .collect();
        found.sort_by_key(|(_, k)| std::cmp::Reverse(k.seen_at));
        Ok(found)
    }
}

impl TemplateRepository for MemoryStore {
    fn template_records(&self) -> PortResult<Vec<TemplateRecord>> {
        Ok(self.templates.lock().unwrap().clone())
    }
    fn save_template(&self, record: &TemplateRecord) -> PortResult<i64> {
        let mut templates = self.templates.lock().unwrap();
        if let Some(t) = templates.iter_mut().find(|t| t.id == record.id) {
            *t = record.clone();
            return Ok(record.id);
        }
        let id = templates.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        templates.push(TemplateRecord {
            id,
            ..record.clone()
        });
        Ok(id)
    }
    fn delete_template(&self, id: i64) -> PortResult<()> {
        self.templates.lock().unwrap().retain(|t| t.id != id);
        Ok(())
    }
    fn reset_builtin_templates(&self) -> PortResult<()> {
        self.templates
            .lock()
            .unwrap()
            .retain(|t| t.builtin.is_none());
        Ok(())
    }
}
