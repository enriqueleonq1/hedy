//! Account assignment rules for protected projects (rule 4 of the spec).
//!
//! Protection is opt-in. A protected project only uses its assigned account,
//! and that account is reserved: it is not offered to any other project.

use std::path::Path;

use super::{Account, AccountId, DomainError, Project};

/// The protected project (other than `except`) that reserves `account`, if any.
pub fn reserved_by<'a>(
    account: &AccountId,
    projects: &'a [Project],
    except: Option<&Path>,
) -> Option<&'a Project> {
    projects.iter().find(|p| {
        p.protected && p.account.as_ref() == Some(account) && Some(p.path.as_path()) != except
    })
}

/// Can `account` be assigned to `project`?
pub fn check_assignment(
    project: &Project,
    account: &AccountId,
    projects: &[Project],
) -> Result<(), DomainError> {
    match reserved_by(account, projects, Some(&project.path)) {
        Some(owner) => Err(DomainError::AccountReserved {
            account: account.clone(),
            project: owner.path.clone(),
        }),
        None => Ok(()),
    }
}

/// Can `project` (with its current account) be marked as protected?
pub fn check_protect(project: &Project, projects: &[Project]) -> Result<(), DomainError> {
    let account = project
        .account
        .as_ref()
        .ok_or(DomainError::ProtectWithoutAccount)?;
    let others: Vec<_> = projects
        .iter()
        .filter(|p| p.path != project.path && p.account.as_ref() == Some(account))
        .map(|p| p.path.clone())
        .collect();
    if others.is_empty() {
        Ok(())
    } else {
        Err(DomainError::AccountShared {
            account: account.clone(),
            others,
        })
    }
}

/// Can `account` be launched in `project` (or outside any project)?
pub fn check_launch(
    project: Option<&Project>,
    account: &AccountId,
    projects: &[Project],
) -> Result<(), DomainError> {
    if let Some(p) = project
        && p.protected
        && let Some(assigned) = &p.account
        && assigned != account
    {
        return Err(DomainError::ProtectedProject {
            project: p.path.clone(),
            assigned: assigned.clone(),
        });
    }
    match reserved_by(account, projects, project.map(|p| p.path.as_path())) {
        Some(owner) => Err(DomainError::AccountReserved {
            account: account.clone(),
            project: owner.path.clone(),
        }),
        None => Ok(()),
    }
}

/// Accounts that may be offered in `project` (or outside any project).
pub fn offerable<'a>(
    project: Option<&Project>,
    accounts: &'a [Account],
    projects: &[Project],
) -> Vec<&'a Account> {
    accounts
        .iter()
        .filter(|a| check_launch(project, &a.id, projects).is_ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Color, Credentials, Provider};

    fn id(s: &str) -> AccountId {
        AccountId::new(s).unwrap()
    }

    fn project(path: &str, account: Option<&str>, protected: bool) -> Project {
        Project {
            account: account.map(id),
            protected,
            ..Project::new(path.into())
        }
    }

    fn account(slug: &str) -> Account {
        Account::new(
            id(slug),
            slug,
            Provider::Kimi,
            Credentials::ApiKey,
            Color::nth_in_palette(0),
        )
        .unwrap()
    }

    #[test]
    fn nothing_is_reserved_by_default() {
        let projects = vec![project("/work", Some("work"), false)];
        let other = project("/hobby", None, false);
        assert!(check_assignment(&other, &id("work"), &projects).is_ok());
        assert!(check_launch(Some(&other), &id("work"), &projects).is_ok());
    }

    #[test]
    fn protected_account_is_reserved_for_its_project() {
        let projects = vec![project("/work", Some("work"), true)];
        let other = project("/hobby", None, false);
        assert!(matches!(
            check_assignment(&other, &id("work"), &projects),
            Err(DomainError::AccountReserved { .. })
        ));
        assert!(check_launch(None, &id("work"), &projects).is_err());
        // The protected project itself may keep or re-assign it.
        assert!(check_assignment(&projects[0], &id("work"), &projects).is_ok());
    }

    #[test]
    fn protected_project_only_launches_its_account() {
        let projects = vec![project("/work", Some("work"), true)];
        assert!(check_launch(Some(&projects[0]), &id("work"), &projects).is_ok());
        assert_eq!(
            check_launch(Some(&projects[0]), &id("personal"), &projects),
            Err(DomainError::ProtectedProject {
                project: "/work".into(),
                assigned: id("work"),
            })
        );
    }

    #[test]
    fn protect_requires_an_exclusive_account() {
        let lone = project("/work", None, false);
        assert_eq!(
            check_protect(&lone, &[]),
            Err(DomainError::ProtectWithoutAccount)
        );

        let projects = vec![
            project("/work", Some("work"), false),
            project("/side", Some("work"), false),
        ];
        assert!(matches!(
            check_protect(&projects[0], &projects),
            Err(DomainError::AccountShared { others, .. }) if others == vec![std::path::PathBuf::from("/side")]
        ));
        assert!(check_protect(&projects[0], &projects[..1]).is_ok());
    }

    #[test]
    fn offerable_hides_reserved_accounts() {
        let accounts = vec![account("work"), account("personal")];
        let projects = vec![project("/work", Some("work"), true)];
        let ids = |p: Option<&Project>| {
            offerable(p, &accounts, &projects)
                .iter()
                .map(|a| a.id.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(None), vec!["personal"]);
        assert_eq!(ids(Some(&projects[0])), vec!["work"]);
    }
}
