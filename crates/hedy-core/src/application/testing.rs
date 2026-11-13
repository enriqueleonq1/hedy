//! In-memory fakes of every port, for use-case tests.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::ports::{
    AccountRepository, CapturedOutput, FileSystem, PortError, PortResult, ProcessRunner,
    ProjectRepository, SecretStore, SettingsRepository,
};
use super::{Hedy, NewAccount};
use crate::domain::{Account, AccountId, LaunchSpec, Project, Provider};

#[derive(Default)]
pub struct MemoryStore {
    accounts: Mutex<Vec<Account>>,
    projects: Mutex<Vec<Project>>,
    settings: Mutex<BTreeMap<String, String>>,
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
}

impl Fixture {
    pub fn new() -> Self {
        let secrets = Arc::new(MemorySecrets::default());
        let runner = Arc::new(FakeRunner::default());
        let fs = Arc::new(FakeFs::default());
        let hedy = Hedy::new(
            Arc::new(MemoryStore::default()),
            secrets.clone(),
            runner.clone(),
            fs.clone(),
        );
        Self {
            hedy,
            secrets,
            runner,
            fs,
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
