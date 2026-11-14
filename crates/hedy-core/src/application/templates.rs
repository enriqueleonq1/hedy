//! Prompt templates: reusable requests for the composer, global or for one
//! project, stored in Hedy's database (never in the repository). The built-in
//! examples live in the code, in the current language, until the user edits
//! or deletes them; restoring them forgets those changes.

use std::path::{Path, PathBuf};

use super::ports::TemplateRecord;
use super::{AppError, AppResult, Hedy};
use crate::domain::template::{builtins, same_name, validate_name, validate_text};
use crate::domain::{PromptTemplate, TemplateId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTemplate {
    /// `None`: every project.
    pub project: Option<PathBuf>,
    pub name: String,
    pub text: String,
}

/// Which templates to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateScope<'a> {
    /// Every template, of every project (the management screen).
    All,
    /// The global ones, plus this project's: what its composer offers.
    For(Option<&'a Path>),
}

impl Hedy {
    /// The project's own templates first, then the global ones, then the
    /// built-in examples.
    pub fn templates(&self, scope: TemplateScope) -> AppResult<Vec<PromptTemplate>> {
        let records = self.store.template_records()?;
        let project = match scope {
            TemplateScope::For(Some(p)) => Some(self.fs.canonicalize(p)?),
            _ => None,
        };
        let wanted = |t: &Option<PathBuf>| match scope {
            TemplateScope::All => true,
            TemplateScope::For(_) => t.is_none() || *t == project,
        };
        let saved = |r: &TemplateRecord| PromptTemplate {
            id: TemplateId::Saved(r.id),
            project: r.project.clone(),
            name: r.name.clone(),
            text: r.text.clone(),
            builtin: r.builtin.clone(),
        };
        let mut own: Vec<PromptTemplate> = records
            .iter()
            .filter(|r| r.builtin.is_none() && !r.deleted && wanted(&r.project))
            .map(saved)
            .collect();
        own.sort_by_key(|t| t.project.is_none());
        for (key, name, text) in builtins() {
            match records.iter().find(|r| r.builtin.as_deref() == Some(key)) {
                Some(r) if r.deleted => {}
                Some(r) => own.push(saved(r)),
                None => own.push(PromptTemplate {
                    id: TemplateId::Builtin(key.to_owned()),
                    project: None,
                    name,
                    text,
                    builtin: Some(key.to_owned()),
                }),
            }
        }
        Ok(own)
    }

    /// A template by name: the project's first, then a global one.
    pub fn template_named(&self, project: Option<&Path>, name: &str) -> AppResult<PromptTemplate> {
        self.templates(TemplateScope::For(project))?
            .into_iter()
            .find(|t| same_name(&t.name, name))
            .ok_or_else(|| AppError::TemplateNotFound(name.trim().to_owned()))
    }

    pub fn add_template(&self, new: NewTemplate) -> AppResult<PromptTemplate> {
        let name = validate_name(&new.name)?;
        let text = validate_text(&new.text)?;
        let project = new
            .project
            .as_deref()
            .map(|p| self.fs.canonicalize(p))
            .transpose()?;
        self.check_name_free(&project, &name, None)?;
        let mut record = TemplateRecord {
            id: 0,
            project,
            name,
            text,
            builtin: None,
            deleted: false,
        };
        record.id = self.store.save_template(&record)?;
        Ok(PromptTemplate {
            id: TemplateId::Saved(record.id),
            project: record.project,
            name: record.name,
            text: record.text,
            builtin: None,
        })
    }

    /// Changes a template's name and text (its scope stays).
    pub fn update_template(
        &self,
        id: &TemplateId,
        name: &str,
        text: &str,
    ) -> AppResult<PromptTemplate> {
        let name = validate_name(name)?;
        let text = validate_text(text)?;
        let current = self.template(id)?;
        self.check_name_free(&current.project, &name, Some(id))?;
        let record = match id {
            TemplateId::Builtin(key) => TemplateRecord {
                id: 0,
                project: None,
                name,
                text,
                builtin: Some(key.clone()),
                deleted: false,
            },
            TemplateId::Saved(row) => TemplateRecord {
                id: *row,
                project: current.project.clone(),
                name,
                text,
                builtin: current.builtin.clone(),
                deleted: false,
            },
        };
        let row = self.store.save_template(&record)?;
        Ok(PromptTemplate {
            id: TemplateId::Saved(row),
            project: record.project,
            name: record.name,
            text: record.text,
            builtin: record.builtin,
        })
    }

    /// Deletes a template. A built-in example can be restored later.
    pub fn remove_template(&self, id: &TemplateId) -> AppResult<()> {
        let current = self.template(id)?;
        match (id, &current.builtin) {
            (TemplateId::Saved(row), None) => self.store.delete_template(*row)?,
            (_, builtin) => {
                let row = match id {
                    TemplateId::Saved(row) => *row,
                    TemplateId::Builtin(_) => 0,
                };
                self.store.save_template(&TemplateRecord {
                    id: row,
                    project: None,
                    name: current.name,
                    text: current.text,
                    builtin: builtin.clone(),
                    deleted: true,
                })?;
            }
        }
        Ok(())
    }

    /// Brings back the built-in examples as they came.
    pub fn restore_builtin_templates(&self) -> AppResult<()> {
        Ok(self.store.reset_builtin_templates()?)
    }

    fn template(&self, id: &TemplateId) -> AppResult<PromptTemplate> {
        self.templates(TemplateScope::All)?
            .into_iter()
            .find(|t| &t.id == id)
            .ok_or_else(|| AppError::TemplateNotFound(id.to_string()))
    }

    /// Names are unique in each scope (a project may reuse a global name:
    /// its own template wins there).
    fn check_name_free(
        &self,
        project: &Option<PathBuf>,
        name: &str,
        except: Option<&TemplateId>,
    ) -> AppResult<()> {
        let taken = self
            .templates(TemplateScope::All)?
            .into_iter()
            .any(|t| &t.project == project && same_name(&t.name, name) && Some(&t.id) != except);
        if taken {
            return Err(AppError::TemplateExists(name.to_owned()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::testing::Fixture;
    use crate::domain::template::placeholders;
    use crate::i18n::{Language, with_language};

    fn names(templates: &[PromptTemplate]) -> Vec<String> {
        templates.iter().map(|t| t.name.clone()).collect()
    }

    fn add(
        f: &Fixture,
        project: Option<&str>,
        name: &str,
        text: &str,
    ) -> AppResult<PromptTemplate> {
        f.hedy.add_template(NewTemplate {
            project: project.map(PathBuf::from),
            name: name.into(),
            text: text.into(),
        })
    }

    #[test]
    fn builtins_come_in_the_current_language() {
        let f = Fixture::new();
        let spanish = f.hedy.templates(TemplateScope::For(None)).unwrap();
        assert_eq!(spanish.len(), 5);
        assert_eq!(spanish[1].name, "Escribir pruebas");
        assert_eq!(spanish[1].id.to_string(), "builtin:tests");
        assert_eq!(placeholders(&spanish[1].text), vec!["archivo"]);
        let english = with_language(Language::English, || {
            f.hedy.templates(TemplateScope::For(None)).unwrap()
        });
        assert_eq!(english[1].name, "Write tests");
    }

    #[test]
    fn project_templates_come_first_and_stay_in_their_project() {
        let f = Fixture::new();
        add(&f, None, "Desplegar", "Prepara el despliegue").unwrap();
        add(
            &f,
            Some("/code/moira"),
            "Migración",
            "Escribe la migración de {tabla}",
        )
        .unwrap();
        add(&f, Some("/code/otro"), "Otra", "x").unwrap();

        let moira = f
            .hedy
            .templates(TemplateScope::For(Some(Path::new("/code/moira"))))
            .unwrap();
        assert_eq!(&names(&moira)[..2], ["Migración", "Desplegar"]);
        assert!(!names(&moira).contains(&"Otra".to_owned()));
        assert_eq!(f.hedy.templates(TemplateScope::All).unwrap().len(), 8);
        assert_eq!(
            f.hedy
                .template_named(Some(Path::new("/code/moira")), "migración")
                .unwrap()
                .text,
            "Escribe la migración de {tabla}"
        );
        assert!(matches!(
            f.hedy.template_named(None, "Migración"),
            Err(AppError::TemplateNotFound(_))
        ));
    }

    #[test]
    fn names_are_unique_per_scope() {
        let f = Fixture::new();
        add(&f, None, "Desplegar", "x").unwrap();
        assert!(matches!(
            add(&f, None, "desplegar", "y"),
            Err(AppError::TemplateExists(_))
        ));
        assert!(matches!(
            add(&f, None, "Escribir pruebas", "y"),
            Err(AppError::TemplateExists(_))
        ));
        add(&f, Some("/code/moira"), "Desplegar", "solo aquí").unwrap();
        assert!(add(&f, None, " ", "x").is_err());
        assert!(add(&f, None, "Vacía", " ").is_err());
    }

    #[test]
    fn builtins_can_be_edited_deleted_and_restored() {
        let f = Fixture::new();
        let tests = TemplateId::Builtin("tests".into());
        let edited = f
            .hedy
            .update_template(&tests, "Pruebas", "Pruebas con pytest para {archivo}")
            .unwrap();
        assert!(matches!(edited.id, TemplateId::Saved(_)));
        assert_eq!(edited.builtin.as_deref(), Some("tests"));
        let list = f.hedy.templates(TemplateScope::For(None)).unwrap();
        assert_eq!(list[1].name, "Pruebas", "keeps its place");
        assert_eq!(list.len(), 5);

        f.hedy.remove_template(&edited.id).unwrap();
        f.hedy
            .remove_template(&TemplateId::Builtin("summary".into()))
            .unwrap();
        assert_eq!(f.hedy.templates(TemplateScope::For(None)).unwrap().len(), 3);

        f.hedy.restore_builtin_templates().unwrap();
        let list = f.hedy.templates(TemplateScope::For(None)).unwrap();
        assert_eq!(list.len(), 5);
        assert_eq!(list[1].name, "Escribir pruebas");
    }

    #[test]
    fn user_templates_are_edited_and_removed() {
        let f = Fixture::new();
        let t = add(&f, None, "Desplegar", "x").unwrap();
        let changed = f.hedy.update_template(&t.id, "Publicar", "y").unwrap();
        assert_eq!(
            (changed.id.clone(), changed.name.as_str()),
            (t.id.clone(), "Publicar")
        );
        f.hedy.remove_template(&t.id).unwrap();
        assert!(f.hedy.template_named(None, "Publicar").is_err());
        assert!(matches!(
            f.hedy.remove_template(&t.id),
            Err(AppError::TemplateNotFound(_))
        ));
        f.hedy.restore_builtin_templates().unwrap();
        assert!(
            f.hedy.template_named(None, "Publicar").is_err(),
            "restore only brings built-ins"
        );
    }
}
