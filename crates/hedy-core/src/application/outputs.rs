//! What a read-only task (a question, a research) writes when it is asked
//! for a document or a page. It goes to the task's own folder in Hedy's data
//! folder (`outputs/<task id>/`), never into the project: Claude Code and
//! the API agent may write only there, and the user opens or saves it from
//! the task. Codex and the ACP agents stay read-only.

use std::path::{Path, PathBuf};

use super::ports::AgentBackend;
use super::{AppError, AppResult, Hedy};
use crate::domain::{OutputFile, Task, TaskId};

impl Hedy {
    pub(super) fn output_folder(&self, task: &TaskId) -> PathBuf {
        self.fs.outputs_dir().join(task.as_str())
    }

    /// The folder a read-only task's agent may write in, created for it;
    /// `None` when the task changes the project instead, or its agent has no
    /// way for Hedy to keep its writes in there.
    pub(super) fn output_for(
        &self,
        task: &Task,
        backend: &AgentBackend,
    ) -> AppResult<Option<PathBuf>> {
        let writes_here = matches!(
            backend,
            AgentBackend::ClaudeCode | AgentBackend::OpenAiCompatible { .. }
        );
        if task.mode.changes_files() || !writes_here {
            return Ok(None);
        }
        let folder = self.output_folder(&task.id);
        self.fs.create_private_dir(&folder)?;
        Ok(Some(folder))
    }

    /// The files the task wrote in its folder, by path.
    pub fn task_outputs(&self, id: &TaskId) -> AppResult<Vec<OutputFile>> {
        self.task(id)?;
        let files = self.fs.files_beneath(&self.output_folder(id))?;
        Ok(files
            .into_iter()
            .map(|(path, bytes)| OutputFile::new(path, bytes))
            .collect())
    }

    /// Where one of the task's files is: only a path the listing gave,
    /// never one the caller made up.
    fn output_path(&self, id: &TaskId, path: &str) -> AppResult<(PathBuf, OutputFile)> {
        let file = self
            .task_outputs(id)?
            .into_iter()
            .find(|f| f.path == path)
            .ok_or_else(|| AppError::NotAnOutput(path.to_owned()))?;
        Ok((self.output_folder(id).join(path), file))
    }

    /// Opens one of the task's files with the system's app for it (a page
    /// in the browser).
    /// Only a type that cannot run ([`OutputFile::opens`]): an agent told by
    /// something it read could write a launcher.
    pub fn open_output(&self, id: &TaskId, path: &str) -> AppResult<()> {
        let (file, listed) = self.output_path(id, path)?;
        if !listed.opens {
            return Err(AppError::OutputNotOpened(path.to_owned()));
        }
        self.open_with_system(&file)
    }

    /// Copies one of the task's files into `folder`, under its own name, or
    /// «name (2).ext» when that is taken: nothing is overwritten. Where it
    /// went.
    pub fn save_output(&self, id: &TaskId, path: &str, folder: &Path) -> AppResult<PathBuf> {
        let (file, _) = self.output_path(id, path)?;
        let name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_owned());
        let (stem, extension) = match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), format!(".{ext}")),
            _ => (name.clone(), String::new()),
        };
        for n in 1..1000 {
            let candidate = match n {
                1 => folder.join(&name),
                n => folder.join(format!("{stem} ({n}){extension}")),
            };
            if self.fs.exists(&candidate) || self.fs.is_symlink(&candidate) {
                continue;
            }
            match self.fs.copy_to_new(&file, &candidate) {
                Ok(()) => return Ok(candidate),
                // Taken meanwhile: the next name.
                Err(_) if self.fs.exists(&candidate) || self.fs.is_symlink(&candidate) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Err(AppError::OutputNotSaved(name))
    }

    /// Deletes what the task wrote (a discarded task, a deleted conversation).
    pub(super) fn remove_outputs(&self, id: &TaskId) -> AppResult<()> {
        Ok(self.fs.remove_dir_all(&self.output_folder(id))?)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::application::testing::{AgentCall, Fixture};
    use crate::application::{AppError, NewTask, ports::FileSystem};
    use crate::domain::{AgentEvent, Decision, OutputFile, TaskId, TaskMode};

    fn question(f: &Fixture, account: &str) -> TaskId {
        f.hedy
            .use_account(&f.dir("/code/agenda"), account, None)
            .unwrap();
        f.hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/agenda"),
                prompt: "Hazme un informe de las citas".into(),
                mode: TaskMode::Research,
                model: None,
                plan_first: false,
            })
            .unwrap()
            .id
    }

    fn write(f: &Fixture, id: &str, path: &str) {
        f.agents.emit(AgentEvent::PermissionRequest {
            request_id: id.into(),
            tool: "Write".into(),
            summary: path.into(),
            input: json!({"file_path": path, "content": "# Citas"}),
        });
    }

    #[test]
    fn a_question_writes_in_its_folder_without_asking_and_nowhere_else() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let id = question(&f, "personal");
        let folder = f.fs.outputs_dir().join(id.as_str());
        assert_eq!(f.agents.last_request().output, Some(folder.clone()));
        f.fs.add_existing(folder.to_str().unwrap());

        write(&f, "w1", &folder.join("informe.md").to_string_lossy());
        write(&f, "w2", "/code/agenda/informe.md");
        write(&f, "w3", "informe.md");
        write(&f, "w4", &folder.join("../t-otra/x.md").to_string_lossy());
        assert_eq!(
            f.agents.calls(),
            vec![
                AgentCall::Answer("w1".into(), true),
                AgentCall::Answer("w2".into(), false),
                AgentCall::Answer("w3".into(), false),
                AgentCall::Answer("w4".into(), false),
            ]
        );
        let events = f.hedy.task_events(&id).unwrap();
        let automatic = events
            .iter()
            .filter(|e| {
                matches!(e, AgentEvent::PermissionResolved { automatic: true, decision, .. }
                    if *decision == Decision::Allow)
            })
            .count();
        assert_eq!(automatic, 1, "no card for the folder of its own");
    }

    #[test]
    fn codex_and_tasks_that_change_files_get_no_folder() {
        let f = Fixture::new();
        f.add_codex("Codex");
        question(&f, "codex");
        assert_eq!(f.agents.last_request().output, None);
        f.add_claude("Personal");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        f.hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/agenda"),
                prompt: "Agrega recordatorios".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: false,
            })
            .unwrap();
        assert_eq!(f.agents.last_request().output, None);
    }

    #[test]
    fn its_files_are_listed_saved_without_overwriting_and_gone_with_it() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let id = question(&f, "personal");
        let folder = f.fs.outputs_dir().join(id.as_str());
        f.fs.write_private_file(&folder.join("informe.md"), b"# Citas")
            .unwrap();
        f.fs.write_private_file(&folder.join("sitio/index.html"), b"<h1>")
            .unwrap();
        assert_eq!(
            f.hedy.task_outputs(&id).unwrap(),
            vec![
                OutputFile::new("informe.md".into(), 7),
                OutputFile::new("sitio/index.html".into(), 4),
            ]
        );
        assert!(f.hedy.task_outputs(&id).unwrap().iter().all(|o| o.opens));

        let downloads = f.dir("/home/e/Descargas");
        let first = f.hedy.save_output(&id, "informe.md", &downloads).unwrap();
        assert_eq!(first, downloads.join("informe.md"));
        let second = f.hedy.save_output(&id, "informe.md", &downloads).unwrap();
        assert_eq!(second, downloads.join("informe (2).md"));
        assert_eq!(f.fs.read_bytes(&second).unwrap(), Some(b"# Citas".to_vec()));
        // Only what the task wrote: never a path made up by the caller.
        for path in ["../../accounts/x", "/etc/passwd", "otro.md"] {
            assert!(matches!(
                f.hedy.save_output(&id, path, &downloads),
                Err(AppError::NotAnOutput(_))
            ));
            assert!(f.hedy.open_output(&id, path).is_err());
        }
        f.hedy.open_output(&id, "sitio/index.html").unwrap();
        // Something that could run is saved, never opened.
        f.fs.write_private_file(&folder.join("lanzar.desktop"), b"[Desktop Entry]")
            .unwrap();
        let outputs = f.hedy.task_outputs(&id).unwrap();
        let launcher = outputs.iter().find(|o| o.path == "lanzar.desktop").unwrap();
        assert!(!launcher.opens);
        assert!(matches!(
            f.hedy.open_output(&id, "lanzar.desktop"),
            Err(AppError::OutputNotOpened(_))
        ));
        f.hedy
            .save_output(&id, "lanzar.desktop", &downloads)
            .unwrap();

        f.hedy.discard_task(&id).unwrap();
        assert!(f.hedy.task_outputs(&id).unwrap().is_empty());
    }
}
