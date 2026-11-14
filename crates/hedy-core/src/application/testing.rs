//! In-memory fakes of every port, for use-case tests.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use super::ports::{
    AccountRepository, AgentLauncher, AgentRequest, AgentSession, CapturedOutput, Clock, EventSink,
    FileSystem, Git, MergeOutcome, PortError, PortResult, ProcessRunner, ProjectRepository,
    RateLimit, SecretStore, SettingsRepository, TaskRepository, UsageRecord, UsageRepository,
};
use super::{Hedy, NewAccount, Ports};
use crate::domain::{
    Account, AccountId, AgentEvent, Commit, FileDiff, LaunchSpec, Project, Provider, Task, TaskId,
    UsageSample,
};

#[derive(Default)]
pub struct MemoryStore {
    accounts: Mutex<Vec<Account>>,
    projects: Mutex<Vec<Project>>,
    settings: Mutex<BTreeMap<String, String>>,
    tasks: Mutex<Vec<Task>>,
    events: Mutex<HashMap<TaskId, Vec<AgentEvent>>>,
    usage: Mutex<Vec<UsageRecord>>,
    limits: Mutex<HashMap<AccountId, RateLimit>>,
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
    fn append_event(&self, id: &TaskId, _at: i64, event: &AgentEvent) -> PortResult<()> {
        self.events
            .lock()
            .unwrap()
            .entry(id.clone())
            .or_default()
            .push(event.clone());
        Ok(())
    }
    fn events(&self, id: &TaskId) -> PortResult<Vec<AgentEvent>> {
        Ok(self
            .events
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .unwrap_or_default())
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
                total.input_tokens += r.sample.input_tokens;
                total.output_tokens += r.sample.output_tokens;
                total.cache_read_tokens += r.sample.cache_read_tokens;
                total.cache_write_tokens += r.sample.cache_write_tokens;
            }
        }
        Ok(total)
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
}

/// Every call ticks one second, so task ids are unique.
pub struct FakeClock(AtomicI64);

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0.fetch_add(1000, Ordering::Relaxed)
    }
}

/// Records git operations instead of running them.
pub struct FakeGit {
    log: Mutex<Vec<String>>,
    excluded: Mutex<Vec<String>>,
    merge: Mutex<MergeOutcome>,
}

impl Default for FakeGit {
    fn default() -> Self {
        Self {
            log: Mutex::default(),
            excluded: Mutex::default(),
            merge: Mutex::new(MergeOutcome::Merged),
        }
    }
}

impl FakeGit {
    pub fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
    pub fn excluded(&self) -> Vec<String> {
        self.excluded.lock().unwrap().clone()
    }
    pub fn set_merge_outcome(&self, outcome: MergeOutcome) {
        *self.merge.lock().unwrap() = outcome;
    }
}

impl Git for FakeGit {
    fn current_branch(&self, _: &Path) -> PortResult<String> {
        Ok("main".into())
    }
    fn head_commit(&self, _: &Path) -> PortResult<String> {
        Ok("abc123".into())
    }
    fn ensure_excluded(&self, _: &Path, pattern: &str) -> PortResult<()> {
        self.excluded.lock().unwrap().push(pattern.into());
        Ok(())
    }
    fn add_worktree(&self, _: &Path, _: &Path, _: &str, _: &str) -> PortResult<()> {
        Ok(())
    }
    fn diff(&self, _: &Path, _: &str) -> PortResult<Vec<FileDiff>> {
        Ok(vec![])
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
        Ok(self.merge.lock().unwrap().clone())
    }
    fn remove_worktree(&self, _: &Path, _: &Path, branch: &str) -> PortResult<()> {
        self.log.lock().unwrap().push(format!("remove {branch}"));
        Ok(())
    }
    fn recent_commits(&self, _: &Path, _: usize) -> PortResult<Vec<Commit>> {
        Ok(vec![])
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
    sink: Mutex<Option<EventSink>>,
    calls: Arc<Mutex<Vec<AgentCall>>>,
    messages: Arc<Mutex<Vec<Option<String>>>>,
}

impl FakeAgents {
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
    pub fn emit(&self, event: AgentEvent) {
        let sink = self.sink.lock().unwrap().clone().expect("no agent started");
        sink(event);
    }
}

struct FakeSession {
    calls: Arc<Mutex<Vec<AgentCall>>>,
    messages: Arc<Mutex<Vec<Option<String>>>>,
}

impl AgentSession for FakeSession {
    fn send(&mut self, text: &str) -> PortResult<()> {
        self.calls
            .lock()
            .unwrap()
            .push(AgentCall::Send(text.into()));
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
        self.requests.lock().unwrap().push(request);
        *self.sink.lock().unwrap() = Some(events);
        Ok(Box::new(FakeSession {
            calls: Arc::clone(&self.calls),
            messages: Arc::clone(&self.messages),
        }))
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
    fn has_api_key(&self, account: &AccountId) -> PortResult<bool> {
        Ok(self.0.lock().unwrap().contains_key(account.as_str()))
    }
    fn delete_api_key(&self, account: &AccountId) -> PortResult<()> {
        self.0.lock().unwrap().remove(account.as_str());
        Ok(())
    }
}

/// Records launches and answers captured runs with a canned reply.
#[derive(Default)]
pub struct FakeRunner {
    interactive: Mutex<Vec<LaunchSpec>>,
    detached: Mutex<Vec<LaunchSpec>>,
    reply: Mutex<Option<CapturedOutput>>,
}

impl FakeRunner {
    pub fn reply(&self, stdout: &str, exit_code: i32) {
        *self.reply.lock().unwrap() = Some(CapturedOutput {
            exit_code: Some(exit_code),
            stdout: stdout.to_owned(),
        });
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
    fn run_captured(&self, _spec: &LaunchSpec) -> PortResult<CapturedOutput> {
        self.reply
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| PortError::new("no reply configured"))
    }
    fn spawn_detached(&self, spec: &LaunchSpec) -> PortResult<()> {
        self.detached.lock().unwrap().push(spec.clone());
        Ok(())
    }
    fn exec(&self, _spec: &LaunchSpec) -> PortError {
        PortError::new("exec is not available in tests")
    }
}

/// Paths are taken as already canonical; account folders live under `/data`.
#[derive(Default)]
pub struct FakeFs {
    dirs: Mutex<HashSet<PathBuf>>,
    files: Mutex<Vec<(PathBuf, String)>>,
    git_roots: Mutex<Vec<PathBuf>>,
}

impl FakeFs {
    pub fn dirs(&self) -> HashSet<PathBuf> {
        self.dirs.lock().unwrap().clone()
    }
    pub fn files(&self) -> Vec<(PathBuf, String)> {
        self.files.lock().unwrap().clone()
    }
    pub fn add_git_root(&self, path: &str) {
        self.git_roots.lock().unwrap().push(path.into());
    }
}

impl FileSystem for FakeFs {
    fn account_folder(&self, id: &AccountId) -> PathBuf {
        Path::new("/data/accounts").join(id.as_str())
    }
    fn create_dir_all(&self, path: &Path) -> PortResult<()> {
        self.dirs.lock().unwrap().insert(path.to_owned());
        Ok(())
    }
    fn write_if_missing(&self, path: &Path, contents: &str) -> PortResult<()> {
        let mut files = self.files.lock().unwrap();
        if !files.iter().any(|(p, _)| p == path) {
            files.push((path.to_owned(), contents.to_owned()));
        }
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

pub struct Fixture {
    pub hedy: Hedy,
    pub secrets: Arc<MemorySecrets>,
    pub runner: Arc<FakeRunner>,
    pub fs: Arc<FakeFs>,
    pub git: Arc<FakeGit>,
    pub agents: Arc<FakeAgents>,
}

impl Fixture {
    pub fn new() -> Self {
        let secrets = Arc::new(MemorySecrets::default());
        let runner = Arc::new(FakeRunner::default());
        let fs = Arc::new(FakeFs::default());
        let git = Arc::new(FakeGit::default());
        let agents = Arc::new(FakeAgents::default());
        let hedy = Hedy::new(Ports {
            store: Arc::new(MemoryStore::default()),
            secrets: secrets.clone(),
            runner: runner.clone(),
            fs: fs.clone(),
            git: git.clone(),
            agents: agents.clone(),
            clock: Arc::new(FakeClock(AtomicI64::new(1_700_000_000_000))),
        });
        Self {
            hedy,
            secrets,
            runner,
            fs,
            git,
            agents,
        }
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

    fn add(&self, name: &str, provider: Provider, api_key: bool) -> Account {
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
