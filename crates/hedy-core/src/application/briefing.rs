//! The `briefing` experiment: what Hedy finds locally for a request before
//! the agent starts (see [`crate::domain::briefing`]).

use std::path::Path;

use super::Hedy;
use crate::domain::briefing::{self, Briefing, MAX_NOTES};
use crate::domain::code::TextQuery;
use crate::domain::{Account, NoteStatus, Task};

/// Matching lines read per name, at most.
const GREP_MAX: usize = 200;
/// Never searched: generated, vendored or Hedy's own.
const SKIPPED_DIRS: &[&str] = &[".git", ".hedy", "node_modules", "target", "dist", "build"];
const SKIPPED_NAMES: &[&str] = &[
    "*.lock",
    "*-lock.json",
    "*.min.js",
    "*.min.css",
    "*.map",
    "*.svg",
];

impl Hedy {
    /// The briefing for the first prompt of `task`, run by `account`; empty
    /// when there is nothing worth saying. Best effort: what cannot be read
    /// is left out.
    pub(super) fn briefing(&self, task: &Task, account: &Account) -> String {
        let dir = task.work_dir();
        let files = self.code.files_at(&dir, "HEAD").unwrap_or_default();
        let mut found = Briefing {
            files: self.matching_files(&dir, &files, &task.prompt),
            test_command: briefing::test_command(&files),
            ..Briefing::default()
        };
        if let Ok(project) = self.project_at(&task.project)
            && !project.memory_off.contains(&account.id)
            && let Ok(notes) = self.store.search_notes(
                Some(&project.path),
                Self::with_global(&project),
                &task.prompt,
            )
        {
            let notes: Vec<_> = notes
                .into_iter()
                .filter(|n| n.status == NoteStatus::Approved)
                .take(MAX_NOTES)
                .collect();
            let ids: Vec<i64> = notes.iter().map(|n| n.id).collect();
            let _ = self
                .store
                .record_uses(&ids, Some(&task.id), self.now_secs());
            found.notes = notes.into_iter().map(|n| n.text).collect();
        }
        found.render()
    }

    /// Files that mention the names in `prompt`: a path names its file (and
    /// its stem is searched, for imports), an identifier is searched as is.
    fn matching_files(
        &self,
        dir: &Path,
        files: &[String],
        prompt: &str,
    ) -> Vec<(String, Vec<String>, usize)> {
        let blocked = self.blocked_files_at(dir);
        let exclude: Vec<&str> = SKIPPED_NAMES
            .iter()
            .copied()
            .chain(blocked.patterns().iter().map(String::as_str))
            .collect();
        let mut hits: Vec<(String, String)> = Vec::new();
        for term in briefing::search_terms(prompt) {
            let mut needle = term.as_str();
            if term.contains('/') || term.contains('.') {
                let named = files
                    .iter()
                    .filter(|f| *f == &term || f.ends_with(&format!("/{term}")));
                hits.extend(named.map(|f| (term.clone(), f.clone())));
                let name = term.rsplit('/').next().unwrap_or(&term);
                needle = name.split('.').next().unwrap_or(name);
                if needle.chars().count() < 3 {
                    continue;
                }
            }
            let query = TextQuery {
                query: needle.to_owned(),
                case_sensitive: true,
                ..TextQuery::default()
            };
            if let Ok((lines, _)) =
                self.code
                    .grep(dir, None, &query, &exclude, SKIPPED_DIRS, GREP_MAX)
            {
                hits.extend(lines.into_iter().map(|(path, _)| (term.clone(), path)));
            }
        }
        briefing::rank_files(&hits)
    }
}

#[cfg(test)]
mod tests {
    use crate::application::NewTask;
    use crate::application::testing::Fixture;
    use crate::domain::{Category, TaskMode};
    use crate::experiments::{Experiment, Experiments, with_experiments};

    #[test]
    fn the_first_prompt_ends_with_the_briefing_only_with_the_experiment() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.fs.add_git_root("/code/shop");
        let project = f.dir("/code/shop");
        f.hedy.use_account(&project, "personal", None).unwrap();
        f.hedy
            .add_note(
                Some(&project),
                "Los precios se muestran como «USD 12.50».",
                Category::Convention,
            )
            .unwrap();
        let start = || {
            f.hedy
                .start_task(NewTask {
                    images: Vec::new(),
                    project: project.clone(),
                    prompt: "Agrega format_price con el formato de los precios".into(),
                    mode: TaskMode::Project,
                    model: None,
                    plan_first: false,
                })
                .unwrap()
        };
        start();
        assert_eq!(
            f.agents.last_request().prompt,
            "Agrega format_price con el formato de los precios"
        );

        let task = with_experiments(Experiments::NONE.with(Experiment::Briefing), start);
        let sent = f.agents.last_request().prompt;
        assert!(
            sent.starts_with(
                "Agrega format_price con el formato de los precios\n\nContexto que preparó Hedy"
            ),
            "{sent}"
        );
        assert!(
            sent.contains("  - Los precios se muestran como «USD 12.50»."),
            "{sent}"
        );
        // What the user wrote stays as they wrote it.
        assert_eq!(
            f.hedy.task(&task.id).unwrap().prompt,
            "Agrega format_price con el formato de los precios"
        );
    }
}
