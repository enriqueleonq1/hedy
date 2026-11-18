//! Fakes of the shipping ports: the folder's git remote and GitHub CLI.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::application::ports::{
    CodeHost, PortError, PortResult, PullRequestSpec, SourceControl, Upstream,
};
use crate::domain::ship::GitFolder;

/// Records pushes, pulls and worktree changes instead of running git.
pub struct FakeSource {
    folder: Mutex<GitFolder>,
    remotes: Mutex<Vec<(String, String)>>,
    /// Remotes of one repo, over `remotes` (which every repo has).
    remotes_of: Mutex<Vec<(PathBuf, String, String)>>,
    upstreams: Mutex<Vec<(String, Upstream)>>,
    default_branch: Mutex<Option<String>>,
    push_error: Mutex<Option<String>>,
    pull_error: Mutex<Option<String>>,
    log: Mutex<Vec<String>>,
}

impl Default for FakeSource {
    fn default() -> Self {
        Self {
            folder: Mutex::new(GitFolder {
                branch: Some("main".into()),
                ..Default::default()
            }),
            remotes: Mutex::default(),
            remotes_of: Mutex::default(),
            upstreams: Mutex::default(),
            default_branch: Mutex::default(),
            push_error: Mutex::default(),
            pull_error: Mutex::default(),
            log: Mutex::default(),
        }
    }
}

impl FakeSource {
    pub fn set_folder(&self, folder: GitFolder) {
        *self.folder.lock().unwrap() = folder;
    }
    /// (name, url) of each remote.
    /// The remotes of `repo` alone (other repos keep [`Self::set_remotes`]').
    pub fn set_remotes_of(&self, repo: &Path, remotes: &[(&str, &str)]) {
        let mut own = self.remotes_of.lock().unwrap();
        own.retain(|(r, _, _)| r != repo);
        own.extend(
            remotes
                .iter()
                .map(|(n, u)| (repo.to_owned(), n.to_string(), u.to_string())),
        );
    }

    pub fn set_remotes(&self, remotes: &[(&str, &str)]) {
        *self.remotes.lock().unwrap() = remotes
            .iter()
            .map(|(n, u)| (n.to_string(), u.to_string()))
            .collect();
    }
    pub fn set_upstream(&self, branch: &str, remote: &str, on: &str, ahead: u32, behind: u32) {
        let mut ups = self.upstreams.lock().unwrap();
        ups.retain(|(b, _)| b != branch);
        ups.push((
            branch.into(),
            Upstream {
                remote: remote.into(),
                branch: on.into(),
                ahead,
                behind,
            },
        ));
    }
    pub fn set_default_branch(&self, branch: &str) {
        *self.default_branch.lock().unwrap() = Some(branch.into());
    }
    /// The next push fails with git's message.
    pub fn fail_push(&self, message: &str) {
        *self.push_error.lock().unwrap() = Some(message.into());
    }
    /// The next pull fails with git's message.
    pub fn fail_pull(&self, message: &str) {
        *self.pull_error.lock().unwrap() = Some(message.into());
    }
    pub fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

impl SourceControl for FakeSource {
    fn folder(&self, _: &Path) -> PortResult<GitFolder> {
        Ok(self.folder.lock().unwrap().clone())
    }
    fn upstream(&self, _: &Path, branch: &str) -> PortResult<Option<Upstream>> {
        Ok(self
            .upstreams
            .lock()
            .unwrap()
            .iter()
            .find(|(b, _)| b == branch)
            .map(|(_, u)| u.clone()))
    }
    fn remotes(&self, _: &Path) -> PortResult<Vec<String>> {
        Ok(self
            .remotes
            .lock()
            .unwrap()
            .iter()
            .map(|(n, _)| n.clone())
            .collect())
    }
    fn remote_url(&self, repo: &Path, remote: &str) -> PortResult<Option<String>> {
        let own = self.remotes_of.lock().unwrap();
        if own.iter().any(|(r, _, _)| r == repo) {
            return Ok(own
                .iter()
                .find(|(r, n, _)| r == repo && n == remote)
                .map(|(_, _, u)| u.clone()));
        }
        Ok(self
            .remotes
            .lock()
            .unwrap()
            .iter()
            .find(|(n, _)| n == remote)
            .map(|(_, u)| u.clone()))
    }
    fn default_branch(&self, _: &Path, _: &str) -> Option<String> {
        self.default_branch.lock().unwrap().clone()
    }
    fn push(
        &self,
        _: &Path,
        remote: &str,
        branch: &str,
        remote_branch: &str,
        set_upstream: bool,
    ) -> PortResult<()> {
        if let Some(message) = self.push_error.lock().unwrap().take() {
            return Err(PortError::new(message));
        }
        let flag = if set_upstream { "-u " } else { "" };
        self.log
            .lock()
            .unwrap()
            .push(format!("push {flag}{remote} {branch}:{remote_branch}"));
        // Pushed: the branch now tracks the remote branch, with nothing ahead.
        self.set_upstream(branch, remote, remote_branch, 0, 0);
        Ok(())
    }
    fn pull_ff_only(&self, _: &Path) -> PortResult<()> {
        if let Some(message) = self.pull_error.lock().unwrap().take() {
            return Err(PortError::new(message));
        }
        self.log.lock().unwrap().push("pull --ff-only".into());
        Ok(())
    }
    fn remove_worktree_keep_branch(&self, _: &Path, path: &Path) -> PortResult<()> {
        self.log
            .lock()
            .unwrap()
            .push(format!("unlink {}", path.display()));
        Ok(())
    }
    fn attach_worktree(&self, _: &Path, path: &Path, branch: &str) -> PortResult<()> {
        self.log
            .lock()
            .unwrap()
            .push(format!("attach {} {branch}", path.display()));
        Ok(())
    }
}

/// GitHub CLI double: installed and logged in as the test says.
#[derive(Default)]
pub struct FakeHost {
    state: Mutex<(bool, bool)>,
    reply: Mutex<Option<PortResult<String>>>,
    created: Mutex<Vec<PullRequestSpec>>,
}

impl FakeHost {
    pub fn set(&self, installed: bool, logged_in: bool) {
        *self.state.lock().unwrap() = (installed, logged_in);
    }
    /// What the next `gh pr create` prints.
    pub fn reply(&self, out: &str) {
        *self.reply.lock().unwrap() = Some(Ok(out.into()));
    }
    /// The next `gh pr create` fails with this message.
    pub fn fail(&self, message: &str) {
        *self.reply.lock().unwrap() = Some(Err(PortError::new(message)));
    }
    pub fn created(&self) -> Vec<PullRequestSpec> {
        self.created.lock().unwrap().clone()
    }
}

impl CodeHost for FakeHost {
    fn installed(&self) -> bool {
        self.state.lock().unwrap().0
    }
    fn logged_in(&self, _: &Path, _: &str) -> bool {
        self.state.lock().unwrap().1
    }
    fn default_branch(&self, _: &Path) -> Option<String> {
        None
    }
    fn create_pull_request(&self, _: &Path, spec: &PullRequestSpec) -> PortResult<String> {
        self.created.lock().unwrap().push(spec.clone());
        self.reply
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| Ok("https://github.com/a/b/pull/1".into()))
    }
}
