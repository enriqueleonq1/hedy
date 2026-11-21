//! Images attached to a message. They are checked (type from the bytes,
//! size, count) before anything is created, kept in Hedy's data folder
//! (`attachments/<task id>/`, private to the user, never in the repository)
//! and go to the agent through its CLI's official input, only when its
//! adapter says it takes them.

use std::path::PathBuf;

use super::{AppError, AppResult, Hedy};
use crate::domain::attachment::{self, display_name, is_stored_name};
use crate::domain::{Account, Attachment, ImageSupport, ImageType, PromptImage, TaskId};

/// An image the user attached, as it came from the composer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewImage {
    /// The file's name, for people.
    pub name: String,
    pub data: Vec<u8>,
}

impl Hedy {
    /// Whether the agent of `account` takes images with a prompt.
    pub fn image_support(&self, account: &str) -> AppResult<ImageSupport> {
        let account = self.account(account)?;
        Ok(self.support_of(&account))
    }

    /// What the account's adapter says; an account that cannot work yet
    /// (an API key missing) takes nothing.
    pub(super) fn support_of(&self, account: &Account) -> ImageSupport {
        self.backend(account)
            .map(|(backend, _)| self.agents.image_support(&backend))
            .unwrap_or(ImageSupport::No)
    }

    /// Checks the images of a message for `account` before anything is
    /// created or sent.
    pub(super) fn check_images(&self, images: &[NewImage], account: &Account) -> AppResult<()> {
        if images.is_empty() {
            return Ok(());
        }
        attachment::check_count(images.len())?;
        for image in images {
            attachment::check_image(&image.name, &image.data)?;
        }
        if !self.support_of(account).accepts() {
            return Err(AppError::ImagesNotAccepted(
                account.provider.agent_label(None),
            ));
        }
        Ok(())
    }

    fn attachments_folder(&self, task: &TaskId) -> PathBuf {
        self.fs.attachments_dir().join(task.as_str())
    }

    /// Keeps the images of a message of `task`, private to the user.
    pub(super) fn store_images(
        &self,
        task: &TaskId,
        images: &[NewImage],
    ) -> AppResult<Vec<Attachment>> {
        if images.is_empty() {
            return Ok(Vec::new());
        }
        let folder = self.attachments_folder(task);
        let stamp = format!("{:x}", self.clock.now_ms());
        images
            .iter()
            .enumerate()
            .map(|(index, image)| {
                let media_type = attachment::check_image(&image.name, &image.data)?;
                let file = format!("{stamp}-{index}.{}", media_type.extension());
                self.fs
                    .write_private_file(&folder.join(&file), &image.data)?;
                Ok(Attachment {
                    name: display_name(&image.name),
                    file,
                    media_type,
                })
            })
            .collect()
    }

    /// The stored files of `attachments`, as the agent gets them.
    pub(super) fn prompt_images(
        &self,
        task: &TaskId,
        attachments: &[Attachment],
    ) -> Vec<PromptImage> {
        let folder = self.attachments_folder(task);
        attachments
            .iter()
            .map(|a| PromptImage {
                path: folder.join(&a.file),
                media_type: a.media_type,
            })
            .collect()
    }

    /// An image of the task's history, to show it again. `None` when it was
    /// deleted (the history still loads) or is not one of the task's images.
    pub fn attachment(&self, task: &TaskId, file: &str) -> AppResult<Option<(ImageType, Vec<u8>)>> {
        // Only a real task's folder, only a name Hedy gave: never a path.
        self.task(task)?;
        if !is_stored_name(file) {
            return Ok(None);
        }
        let Some(bytes) = self
            .fs
            .read_bytes(&self.attachments_folder(task).join(file))?
        else {
            return Ok(None);
        };
        Ok(ImageType::sniff(&bytes).map(|kind| (kind, bytes)))
    }

    /// Deletes the task's images (a deleted conversation).
    pub(super) fn remove_attachments(&self, task: &TaskId) -> AppResult<()> {
        Ok(self.fs.remove_dir_all(&self.attachments_folder(task))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::testing::{AgentCall, Fixture};
    use crate::application::{NewConversation, NewTask, Setting};
    use crate::domain::{AgentEvent, DomainError, TaskMode};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    fn png(name: &str) -> NewImage {
        NewImage {
            name: name.into(),
            data: PNG.to_vec(),
        }
    }

    fn project(f: &Fixture) {
        f.add_claude("Personal");
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
    }

    fn ask(f: &Fixture, images: Vec<NewImage>) -> AppResult<crate::domain::Task> {
        f.hedy.start_task(NewTask {
            project: f.dir("/code/agenda"),
            prompt: "¿Qué falla aquí?".into(),
            mode: TaskMode::Ask,
            model: None,
            plan_first: false,
            permissions: None,
            images,
        })
    }

    fn finished() -> AgentEvent {
        AgentEvent::Finished {
            ok: true,
            summary: "Listo".into(),
            duration_ms: None,
        }
    }

    #[test]
    fn a_new_task_keeps_its_images_privately_and_sends_them() {
        let f = Fixture::new();
        project(&f);
        let task = ask(&f, vec![png("/home/e/captura.png")]).unwrap();

        let folder = PathBuf::from("/data/attachments").join(task.id.as_str());
        assert_eq!(task.images.len(), 1);
        let stored = &task.images[0];
        assert_eq!(stored.name, "captura.png");
        assert_eq!(stored.media_type, ImageType::Png);
        assert_eq!(f.fs.blob_paths(), vec![folder.join(&stored.file)]);
        assert!(f.fs.dirs().contains(&folder), "a private folder");

        let request = f.agents.last_request();
        assert_eq!(request.images.len(), 1);
        assert_eq!(request.images[0].path, folder.join(&stored.file));
        assert_eq!(
            f.hedy.task_events(&task.id).unwrap()[0],
            AgentEvent::UserMessage {
                text: "¿Qué falla aquí?".into(),
                images: task.images.clone(),
            }
        );
        // The stored task remembers them for the next phases.
        assert_eq!(f.hedy.task(&task.id).unwrap().images, task.images);
    }

    #[test]
    fn bad_images_and_agents_without_images_create_nothing() {
        let f = Fixture::new();
        project(&f);
        let svg = NewImage {
            name: "logo.svg".into(),
            data: b"<svg/>".to_vec(),
        };
        assert_eq!(
            ask(&f, vec![svg]),
            Err(AppError::Domain(DomainError::UnsupportedImage(
                "logo.svg".into()
            )))
        );
        let five = (0..5).map(|i| png(&format!("{i}.png"))).collect();
        assert_eq!(
            ask(&f, five),
            Err(AppError::Domain(DomainError::TooManyImages(4)))
        );
        f.agents.set_image_support(ImageSupport::No);
        assert!(matches!(
            ask(&f, vec![png("a.png")]),
            Err(AppError::ImagesNotAccepted(_))
        ));
        assert_eq!(f.hedy.image_support("personal").unwrap(), ImageSupport::No);
        assert!(f.agents.requests().is_empty());
        assert!(f.fs.blob_paths().is_empty());
    }

    #[test]
    fn follow_ups_send_images_now_but_never_queue_them() {
        let f = Fixture::new();
        project(&f);
        let task = ask(&f, Vec::new()).unwrap();
        assert_eq!(
            f.hedy
                .queue_message_with(&task.id, "mira esto", vec![png("a.png")]),
            Err(AppError::ImagesWhileQueued)
        );
        assert!(f.hedy.queued(&task.id).unwrap().is_empty());

        f.agents.emit(finished());
        f.hedy
            .queue_message_with(&task.id, "mira esto", vec![png("a.png")])
            .unwrap();
        assert_eq!(
            f.agents.calls().last(),
            Some(&AgentCall::Send("mira esto".into()))
        );
        let sent = f.agents.sent_images();
        assert_eq!(sent.last().map(Vec::len), Some(1));
        let events = f.hedy.task_events(&task.id).unwrap();
        let Some(AgentEvent::UserMessage { images, .. }) = events
            .iter()
            .rev()
            .find(|e| matches!(e, AgentEvent::UserMessage { .. }))
        else {
            panic!("no user message");
        };
        assert_eq!(images.len(), 1);
        assert_eq!(
            sent[0][0].path.file_name().unwrap(),
            images[0].file.as_str()
        );
    }

    #[test]
    fn history_images_load_until_deleted_and_never_by_path() {
        let f = Fixture::new();
        project(&f);
        let task = ask(&f, vec![png("a.png")]).unwrap();
        let file = task.images[0].file.clone();
        assert_eq!(
            f.hedy.attachment(&task.id, &file).unwrap(),
            Some((ImageType::Png, PNG.to_vec()))
        );
        for bad in ["../../etc/passwd", "/etc/passwd", "a/b.png"] {
            assert_eq!(f.hedy.attachment(&task.id, bad).unwrap(), None);
        }
        assert!(
            f.hedy.attachment(&TaskId::new("../x"), &file).is_err(),
            "only a real task's folder"
        );
        f.fs.delete_blob(&format!("/data/attachments/{}/{file}", task.id));
        assert_eq!(f.hedy.attachment(&task.id, &file).unwrap(), None);
        // The history still loads.
        assert!(!f.hedy.task_events(&task.id).unwrap().is_empty());
    }

    #[test]
    fn agents_without_images_are_told_instead() {
        let f = Fixture::new();
        project(&f);
        let task = ask(&f, vec![png("a.png")]).unwrap();
        f.agents.emit(finished());
        // A later phase or a handoff whose agent takes no images.
        f.agents.set_image_support(ImageSupport::No);
        let mut stored = f.hedy.task(&task.id).unwrap();
        f.hedy
            .run_stage(&mut stored, 0, "Sigue".into(), None)
            .unwrap();
        let request = f.agents.last_request();
        assert!(request.images.is_empty());
        assert!(
            request.prompt.contains("1 imagen(es)"),
            "{}",
            request.prompt
        );
    }

    #[test]
    fn conversations_take_images_and_delete_them_with_the_chat() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.hedy
            .set_setting(Setting::DefaultAccount, Some("personal"))
            .unwrap();
        let chat = f
            .hedy
            .start_conversation(NewConversation {
                prompt: "¿Qué planta es?".into(),
                mode: TaskMode::Ask,
                account: None,
                model: None,
                images: vec![png("planta.png")],
            })
            .unwrap();
        assert_eq!(chat.images.len(), 1);
        assert_eq!(f.agents.last_request().images.len(), 1);
        f.agents.emit(finished());
        f.hedy.delete_conversation(&chat.id).unwrap();
        assert!(
            f.fs.deleted_trees()
                .contains(&PathBuf::from("/data/attachments").join(chat.id.as_str()))
        );
    }
}
