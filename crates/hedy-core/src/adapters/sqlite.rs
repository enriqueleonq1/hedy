use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::application::ports::{
    AccountRepository, PortError, PortResult, ProjectRepository, RateLimit, SettingsRepository,
    TaskRepository, UsageRecord, UsageRepository,
};
use crate::domain::{
    Account, AccountId, AgentEvent, Color, ConfigFolder, Credentials, Project, Task, TaskId,
    UsageSample,
};

/// Schema migrations, applied in order. `PRAGMA user_version` stores how many ran.
const MIGRATIONS: &[&str] = &[
    r#"
    CREATE TABLE accounts (
        id             TEXT PRIMARY KEY,
        name           TEXT NOT NULL,
        provider       TEXT NOT NULL,
        credential     TEXT NOT NULL CHECK (credential IN ('subscription', 'api_key')),
        -- NULL for API keys and for the CLI's default folder.
        config_folder  TEXT,
        color          TEXT NOT NULL,
        position       INTEGER NOT NULL
    );
    CREATE TABLE projects (
        path        TEXT PRIMARY KEY,
        name        TEXT NOT NULL,
        account_id  TEXT REFERENCES accounts(id) ON DELETE RESTRICT,
        protected   INTEGER NOT NULL DEFAULT 0,
        color       TEXT
    );
    CREATE TABLE settings (
        key    TEXT PRIMARY KEY,
        value  TEXT NOT NULL
    );
"#,
    r#"
    CREATE TABLE tasks (
        id          TEXT PRIMARY KEY,
        project     TEXT NOT NULL,
        created_at  INTEGER NOT NULL,
        -- The whole task as JSON; columns above are for lookups.
        data        TEXT NOT NULL
    );
    CREATE INDEX tasks_by_project ON tasks (project, created_at);
    CREATE TABLE task_events (
        task_id  TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
        seq      INTEGER NOT NULL,
        at       INTEGER NOT NULL,
        event    TEXT NOT NULL,
        PRIMARY KEY (task_id, seq)
    );
    CREATE TABLE usage (
        id                  INTEGER PRIMARY KEY,
        at                  INTEGER NOT NULL,
        account_id          TEXT NOT NULL,
        project             TEXT NOT NULL,
        task_id             TEXT NOT NULL,
        input_tokens        INTEGER NOT NULL,
        output_tokens       INTEGER NOT NULL,
        cache_read_tokens   INTEGER NOT NULL,
        cache_write_tokens  INTEGER NOT NULL,
        cost_usd            REAL
    );
    CREATE INDEX usage_by_account ON usage (account_id, at);
    CREATE TABLE rate_limits (
        account_id   TEXT PRIMARY KEY,
        status       TEXT NOT NULL,
        utilization  REAL,
        resets_at    INTEGER,
        at           INTEGER NOT NULL
    );
"#,
];

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

fn db_error(err: rusqlite::Error) -> PortError {
    PortError::new(format!("error de la base de datos: {err}"))
}

impl SqliteStore {
    pub fn open(path: &Path) -> PortResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| PortError::new(format!("no se pudo crear {}: {e}", dir.display())))?;
        }
        Self::from_connection(Connection::open(path).map_err(db_error)?)
    }

    pub fn in_memory() -> PortResult<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(db_error)?)
    }

    fn from_connection(mut conn: Connection) -> PortResult<Self> {
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(db_error)?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> PortResult<T> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        f(&conn).map_err(db_error)
    }
}

fn migrate(conn: &mut Connection) -> PortResult<()> {
    let applied: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(db_error)?;
    let applied = usize::try_from(applied).unwrap_or(usize::MAX);
    if applied > MIGRATIONS.len() {
        return Err(PortError::new(
            "la base de datos es de una versión más nueva de Hedy",
        ));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(applied) {
        let tx = conn.transaction().map_err(db_error)?;
        tx.execute_batch(sql).map_err(db_error)?;
        tx.pragma_update(None, "user_version", i as i64 + 1)
            .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
    }
    Ok(())
}

/// Turns a stored value that fails domain validation into a row error.
fn invalid<E: std::fmt::Display>(column: usize) -> impl Fn(E) -> rusqlite::Error {
    move |e| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            e.to_string().into(),
        )
    }
}

fn account_from_row(row: &Row) -> rusqlite::Result<Account> {
    let id = AccountId::new(row.get::<_, String>(0)?).map_err(invalid(0))?;
    let provider = row.get::<_, String>(2)?.parse().map_err(invalid(2))?;
    let credentials = match row.get::<_, String>(3)?.as_str() {
        "api_key" => Credentials::ApiKey,
        _ => Credentials::Subscription(match row.get::<_, Option<String>>(4)? {
            Some(path) => ConfigFolder::Managed(PathBuf::from(path)),
            None => ConfigFolder::Default,
        }),
    };
    let color: Color = row.get::<_, String>(5)?.parse().map_err(invalid(5))?;
    Account::new(id, row.get::<_, String>(1)?, provider, credentials, color).map_err(invalid(1))
}

impl AccountRepository for SqliteStore {
    fn list_accounts(&self) -> PortResult<Vec<Account>> {
        self.with(|c| {
            c.prepare(
                "SELECT id, name, provider, credential, config_folder, color
                 FROM accounts ORDER BY position",
            )?
            .query_map([], account_from_row)?
            .collect()
        })
    }

    fn save_account(&self, account: &Account) -> PortResult<()> {
        let (credential, folder) = match &account.credentials {
            Credentials::ApiKey => ("api_key", None),
            Credentials::Subscription(ConfigFolder::Default) => ("subscription", None),
            Credentials::Subscription(ConfigFolder::Managed(p)) => {
                ("subscription", Some(p.to_string_lossy().into_owned()))
            }
        };
        self.with(|c| {
            c.execute(
                "INSERT INTO accounts (id, name, provider, credential, config_folder, color, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, (SELECT COALESCE(MAX(position), 0) + 1 FROM accounts))
                 ON CONFLICT (id) DO UPDATE SET
                    name = excluded.name, provider = excluded.provider,
                    credential = excluded.credential, config_folder = excluded.config_folder,
                    color = excluded.color",
                params![
                    account.id.as_str(),
                    account.name,
                    account.provider.as_str(),
                    credential,
                    folder,
                    account.color.to_string(),
                ],
            )
            .map(drop)
        })
    }

    fn delete_account(&self, id: &AccountId) -> PortResult<()> {
        self.with(|c| {
            c.execute("DELETE FROM accounts WHERE id = ?1", [id.as_str()])
                .map(drop)
        })
    }
}

fn project_from_row(row: &Row) -> rusqlite::Result<Project> {
    let account = row
        .get::<_, Option<String>>(2)?
        .map(AccountId::new)
        .transpose()
        .map_err(invalid(2))?;
    let color = row
        .get::<_, Option<String>>(4)?
        .map(|c| c.parse::<Color>())
        .transpose()
        .map_err(invalid(4))?;
    Ok(Project {
        path: PathBuf::from(row.get::<_, String>(0)?),
        name: row.get(1)?,
        account,
        protected: row.get(3)?,
        color,
    })
}

impl ProjectRepository for SqliteStore {
    fn list_projects(&self) -> PortResult<Vec<Project>> {
        self.with(|c| {
            c.prepare(
                "SELECT path, name, account_id, protected, color FROM projects ORDER BY name",
            )?
            .query_map([], project_from_row)?
            .collect()
        })
    }

    fn save_project(&self, project: &Project) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO projects (path, name, account_id, protected, color)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (path) DO UPDATE SET
                    name = excluded.name, account_id = excluded.account_id,
                    protected = excluded.protected, color = excluded.color",
                params![
                    project.path.to_string_lossy(),
                    project.name,
                    project.account.as_ref().map(AccountId::as_str),
                    project.protected,
                    project.color.map(|c| c.to_string()),
                ],
            )
            .map(drop)
        })
    }
    fn delete_project(&self, path: &Path) -> PortResult<()> {
        self.delete_project_row(path)
    }
}

impl SqliteStore {
    fn delete_project_row(&self, path: &Path) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "DELETE FROM projects WHERE path = ?1",
                [path.to_string_lossy()],
            )
            .map(drop)
        })
    }
}

fn json_error(err: serde_json::Error) -> PortError {
    PortError::new(format!("error de la base de datos (JSON): {err}"))
}

impl TaskRepository for SqliteStore {
    fn save_task(&self, task: &Task) -> PortResult<()> {
        let data = serde_json::to_string(task).map_err(json_error)?;
        self.with(|c| {
            c.execute(
                "INSERT INTO tasks (id, project, created_at, data) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (id) DO UPDATE SET data = excluded.data",
                params![
                    task.id.as_str(),
                    task.project.to_string_lossy(),
                    task.created_at,
                    data
                ],
            )
            .map(drop)
        })
    }

    fn task(&self, id: &TaskId) -> PortResult<Option<Task>> {
        let data: Option<String> = self.with(|c| {
            c.query_row("SELECT data FROM tasks WHERE id = ?1", [id.as_str()], |r| {
                r.get(0)
            })
            .optional()
        })?;
        data.map(|d| serde_json::from_str(&d).map_err(json_error))
            .transpose()
    }

    fn tasks(&self, project: &Path) -> PortResult<Vec<Task>> {
        let rows: Vec<String> = self.with(|c| {
            c.prepare(
                "SELECT data FROM tasks WHERE project = ?1 ORDER BY created_at DESC, id DESC",
            )?
            .query_map([project.to_string_lossy()], |r| r.get(0))?
            .collect()
        })?;
        rows.iter()
            .map(|d| serde_json::from_str(d).map_err(json_error))
            .collect()
    }

    fn append_event(&self, id: &TaskId, at: i64, event: &AgentEvent) -> PortResult<()> {
        let data = serde_json::to_string(event).map_err(json_error)?;
        self.with(|c| {
            c.execute(
                "INSERT INTO task_events (task_id, seq, at, event)
                 VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM task_events WHERE task_id = ?1), ?2, ?3)",
                params![id.as_str(), at, data],
            )
            .map(drop)
        })
    }

    fn events(&self, id: &TaskId) -> PortResult<Vec<AgentEvent>> {
        let rows: Vec<String> = self.with(|c| {
            c.prepare("SELECT event FROM task_events WHERE task_id = ?1 ORDER BY seq")?
                .query_map([id.as_str()], |r| r.get(0))?
                .collect()
        })?;
        rows.iter()
            .map(|d| serde_json::from_str(d).map_err(json_error))
            .collect()
    }
}

impl UsageRepository for SqliteStore {
    fn record_usage(&self, r: &UsageRecord) -> PortResult<()> {
        let s = &r.sample;
        self.with(|c| {
            c.execute(
                "INSERT INTO usage (at, account_id, project, task_id, input_tokens, output_tokens,
                                    cache_read_tokens, cache_write_tokens, cost_usd)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    r.at,
                    r.account.as_str(),
                    r.project.to_string_lossy(),
                    r.task.as_str(),
                    s.input_tokens as i64,
                    s.output_tokens as i64,
                    s.cache_read_tokens as i64,
                    s.cache_write_tokens as i64,
                    s.cost_usd,
                ],
            )
            .map(drop)
        })
    }

    fn usage_since(&self, account: &AccountId, since: i64) -> PortResult<UsageSample> {
        self.with(|c| {
            c.query_row(
                "SELECT COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0),
                        COALESCE(SUM(cache_read_tokens), 0), COALESCE(SUM(cache_write_tokens), 0),
                        SUM(cost_usd)
                 FROM usage WHERE account_id = ?1 AND at >= ?2",
                params![account.as_str(), since],
                |r| {
                    Ok(UsageSample {
                        input_tokens: r.get::<_, i64>(0)? as u64,
                        output_tokens: r.get::<_, i64>(1)? as u64,
                        cache_read_tokens: r.get::<_, i64>(2)? as u64,
                        cache_write_tokens: r.get::<_, i64>(3)? as u64,
                        cost_usd: r.get(4)?,
                    })
                },
            )
        })
    }

    fn set_rate_limit(&self, account: &AccountId, l: &RateLimit) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO rate_limits (account_id, status, utilization, resets_at, at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (account_id) DO UPDATE SET status = excluded.status,
                    utilization = excluded.utilization, resets_at = excluded.resets_at, at = excluded.at",
                params![account.as_str(), l.status, l.utilization, l.resets_at, l.at],
            )
            .map(drop)
        })
    }

    fn rate_limit(&self, account: &AccountId) -> PortResult<Option<RateLimit>> {
        self.with(|c| {
            c.query_row(
                "SELECT status, utilization, resets_at, at FROM rate_limits WHERE account_id = ?1",
                [account.as_str()],
                |r| {
                    Ok(RateLimit {
                        status: r.get(0)?,
                        utilization: r.get(1)?,
                        resets_at: r.get(2)?,
                        at: r.get(3)?,
                    })
                },
            )
            .optional()
        })
    }
}

impl SettingsRepository for SqliteStore {
    fn get_setting(&self, key: &str) -> PortResult<Option<String>> {
        self.with(|c| {
            c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
        })
    }

    fn set_setting(&self, key: &str, value: Option<&str>) -> PortResult<()> {
        self.with(|c| {
            match value {
                Some(v) => c.execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                    [key, v],
                ),
                None => c.execute("DELETE FROM settings WHERE key = ?1", [key]),
            }
            .map(drop)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Provider;

    fn account(id: &str, provider: Provider, credentials: Credentials) -> Account {
        Account::new(
            AccountId::new(id).unwrap(),
            id.to_uppercase(),
            provider,
            credentials,
            Color::nth_in_palette(1),
        )
        .unwrap()
    }

    #[test]
    fn accounts_round_trip_in_insertion_order() {
        let store = SqliteStore::in_memory().unwrap();
        let accounts = [
            account(
                "work",
                Provider::Claude,
                Credentials::Subscription(ConfigFolder::Default),
            ),
            account(
                "personal",
                Provider::Claude,
                Credentials::Subscription(ConfigFolder::Managed("/acc/personal".into())),
            ),
            account("kimi", Provider::Kimi, Credentials::ApiKey),
        ];
        for a in &accounts {
            store.save_account(a).unwrap();
        }
        assert_eq!(store.list_accounts().unwrap(), accounts);

        let mut renamed = accounts[0].clone();
        renamed.name = "Trabajo".into();
        store.save_account(&renamed).unwrap();
        assert_eq!(
            store.list_accounts().unwrap()[0],
            renamed,
            "update keeps position"
        );

        store.delete_account(&accounts[2].id).unwrap();
        assert_eq!(store.list_accounts().unwrap().len(), 2);
    }

    #[test]
    fn projects_and_settings_round_trip() {
        let store = SqliteStore::in_memory().unwrap();
        let acc = account(
            "work",
            Provider::Codex,
            Credentials::Subscription(ConfigFolder::Default),
        );
        store.save_account(&acc).unwrap();
        let project = Project {
            account: Some(acc.id.clone()),
            protected: true,
            color: Some(Color::nth_in_palette(3)),
            ..Project::new("/code/homming".into())
        };
        store.save_project(&project).unwrap();
        store.save_project(&project).unwrap();
        assert_eq!(store.list_projects().unwrap(), vec![project.clone()]);

        // An account in use cannot be deleted behind the application's back.
        assert!(store.delete_account(&acc.id).is_err());

        assert_eq!(store.get_setting("default_account").unwrap(), None);
        store.set_setting("default_account", Some("work")).unwrap();
        store.set_setting("default_account", Some("other")).unwrap();
        assert_eq!(
            store.get_setting("default_account").unwrap().as_deref(),
            Some("other")
        );
        store.set_setting("default_account", None).unwrap();
        assert_eq!(store.get_setting("default_account").unwrap(), None);

        store.delete_project(&project.path).unwrap();
        assert!(store.list_projects().unwrap().is_empty());
        store.delete_account(&acc.id).unwrap();
    }

    #[test]
    fn tasks_events_and_usage_round_trip() {
        use crate::domain::{TaskMode, TaskStatus, Workspace};
        let store = SqliteStore::in_memory().unwrap();
        let task = |id: &str, at: i64| Task {
            id: TaskId::new(id),
            project: "/code/moira".into(),
            mode: TaskMode::Project,
            title: "Recordatorios".into(),
            prompt: "Agrega recordatorios".into(),
            account: AccountId::new("personal").unwrap(),
            model: None,
            workspace: Some(Workspace {
                worktree: "/code/moira/.hedy/worktrees/t1".into(),
                branch: "hedy/recordatorios-t1".into(),
                base_branch: "main".into(),
                base_commit: "abc".into(),
            }),
            session_id: None,
            status: TaskStatus::Running,
            created_at: at,
        };
        let (old, new) = (task("t1", 10), task("t2", 20));
        store.save_task(&old).unwrap();
        store.save_task(&new).unwrap();
        let mut updated = old.clone();
        updated.status = TaskStatus::Review;
        store.save_task(&updated).unwrap();
        assert_eq!(store.task(&old.id).unwrap(), Some(updated.clone()));
        assert_eq!(
            store.tasks(Path::new("/code/moira")).unwrap(),
            vec![new.clone(), updated]
        );

        let events = [
            AgentEvent::UserMessage {
                text: "hola".into(),
            },
            AgentEvent::FileEdited {
                path: "a.py".into(),
            },
        ];
        for e in &events {
            store.append_event(&old.id, 1, e).unwrap();
        }
        assert_eq!(store.events(&old.id).unwrap(), events);
        assert!(store.events(&new.id).unwrap().is_empty());

        let account = AccountId::new("personal").unwrap();
        for (at, tokens) in [(5, 100), (50, 10), (60, 1)] {
            store
                .record_usage(&UsageRecord {
                    at,
                    account: account.clone(),
                    project: "/code/moira".into(),
                    task: old.id.clone(),
                    sample: UsageSample {
                        input_tokens: tokens,
                        output_tokens: 1,
                        cost_usd: Some(0.5),
                        ..Default::default()
                    },
                })
                .unwrap();
        }
        let since = store.usage_since(&account, 40).unwrap();
        assert_eq!(
            (since.input_tokens, since.output_tokens, since.cost_usd),
            (11, 2, Some(1.0))
        );

        assert_eq!(store.rate_limit(&account).unwrap(), None);
        let limit = RateLimit {
            status: "allowed_warning".into(),
            utilization: Some(0.92),
            resets_at: Some(99),
            at: 7,
        };
        store.set_rate_limit(&account, &limit).unwrap();
        assert_eq!(store.rate_limit(&account).unwrap(), Some(limit));
    }

    #[test]
    fn migrations_are_idempotent_on_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested/hedy.db");
        SqliteStore::open(&path).unwrap();
        let store = SqliteStore::open(&path).unwrap();
        assert!(store.list_accounts().unwrap().is_empty());
    }
}
