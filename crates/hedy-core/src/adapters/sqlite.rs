use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::application::ports::{
    AccountRepository, MemoryRepository, NewNote, NewSkill, NoteRevision, PlanUsageRecord,
    PortError, PortResult, ProjectRepository, RateLimit, SettingsRepository, SkillRepository,
    SkillRevision, StoreHealth, TaskRepository, UsageRecord, UsageRepository,
};
use crate::domain::{
    Account, AccountId, AgentEvent, Category, Color, ConfigFolder, Credentials, Note, NoteStatus,
    PlanUsage, Project, Skill, SkillStatus, Task, TaskId, UsageProblem, UsageSample,
};
use crate::tr;

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
    r#"
    -- Who plans, implements, writes tests and reviews (JSON).
    ALTER TABLE projects ADD COLUMN phases TEXT;
"#,
    r#"
    -- Accounts that do not read the project's memory (JSON list).
    ALTER TABLE projects ADD COLUMN memory_off TEXT;
    CREATE TABLE notes (
        id          INTEGER PRIMARY KEY,
        -- NULL: a global note.
        project     TEXT,
        text        TEXT NOT NULL,
        category    TEXT NOT NULL,
        author      TEXT NOT NULL,
        status      TEXT NOT NULL CHECK (status IN ('proposed', 'approved')),
        created_at  INTEGER NOT NULL,
        uses        INTEGER NOT NULL DEFAULT 0,
        last_used   INTEGER
    );
    CREATE INDEX notes_by_project ON notes (project);
    CREATE VIRTUAL TABLE notes_fts USING fts5(
        text, content = 'notes', content_rowid = 'id', tokenize = 'unicode61 remove_diacritics 2'
    );
    CREATE TRIGGER notes_fts_insert AFTER INSERT ON notes BEGIN
        INSERT INTO notes_fts (rowid, text) VALUES (new.id, new.text);
    END;
    CREATE TRIGGER notes_fts_delete AFTER DELETE ON notes BEGIN
        INSERT INTO notes_fts (notes_fts, rowid, text) VALUES ('delete', old.id, old.text);
    END;
    CREATE TRIGGER notes_fts_update AFTER UPDATE OF text ON notes BEGIN
        INSERT INTO notes_fts (notes_fts, rowid, text) VALUES ('delete', old.id, old.text);
        INSERT INTO notes_fts (rowid, text) VALUES (new.id, new.text);
    END;
    CREATE TABLE note_uses (
        note_id  INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
        task_id  TEXT,
        at       INTEGER NOT NULL
    );
    CREATE INDEX note_uses_by_task ON note_uses (task_id);
    CREATE TABLE note_dismissed_pairs (
        a  INTEGER NOT NULL,
        b  INTEGER NOT NULL,
        PRIMARY KEY (a, b)
    );
"#,
    r#"
    ALTER TABLE projects ADD COLUMN client TEXT;
    CREATE INDEX usage_by_project ON usage (project, at);
"#,
    r#"
    -- Forgetting a note deletes its uses.
    CREATE INDEX note_uses_by_note ON note_uses (note_id);
"#,
    r#"
    -- A stable subject (`arquitectura/pagos`): a proposal under the topic of an
    -- approved note is a change to that note.
    ALTER TABLE notes ADD COLUMN topic TEXT;
    -- When an approved note was last replaced by a newer version.
    ALTER TABLE notes ADD COLUMN updated_at INTEGER;
    CREATE INDEX notes_by_topic ON notes (project, topic);
    -- The full-text index covers topics too, so "pagos" finds `arquitectura/pagos`.
    DROP TRIGGER notes_fts_insert;
    DROP TRIGGER notes_fts_delete;
    DROP TRIGGER notes_fts_update;
    DROP TABLE notes_fts;
    CREATE VIRTUAL TABLE notes_fts USING fts5(
        text, topic, content = 'notes', content_rowid = 'id',
        tokenize = 'unicode61 remove_diacritics 2'
    );
    INSERT INTO notes_fts (notes_fts) VALUES ('rebuild');
    CREATE TRIGGER notes_fts_insert AFTER INSERT ON notes BEGIN
        INSERT INTO notes_fts (rowid, text, topic) VALUES (new.id, new.text, new.topic);
    END;
    CREATE TRIGGER notes_fts_delete AFTER DELETE ON notes BEGIN
        INSERT INTO notes_fts (notes_fts, rowid, text, topic)
            VALUES ('delete', old.id, old.text, old.topic);
    END;
    CREATE TRIGGER notes_fts_update AFTER UPDATE OF text, topic ON notes BEGIN
        INSERT INTO notes_fts (notes_fts, rowid, text, topic)
            VALUES ('delete', old.id, old.text, old.topic);
        INSERT INTO notes_fts (rowid, text, topic) VALUES (new.id, new.text, new.topic);
    END;
"#,
    r#"
    -- Learned skills: reusable procedures agents propose and the user approves.
    CREATE TABLE skills (
        id          INTEGER PRIMARY KEY,
        -- NULL: a global skill.
        project     TEXT,
        name        TEXT NOT NULL,
        description TEXT NOT NULL,
        body        TEXT NOT NULL,
        status      TEXT NOT NULL CHECK (status IN ('proposed', 'approved')),
        author      TEXT NOT NULL,
        created_at  INTEGER NOT NULL,
        updated_at  INTEGER,
        source_task TEXT
    );
    CREATE INDEX skills_by_project ON skills (project, name);
"#,
    r#"
    -- Messages written while an agent works, sent when its turn ends.
    CREATE TABLE queued_messages (
        id          INTEGER PRIMARY KEY,
        task_id     TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
        text        TEXT NOT NULL,
        created_at  INTEGER NOT NULL,
        -- NULL, or why it waits for the user.
        held        TEXT CHECK (held IN ('failed', 'stopped', 'limit'))
    );
    CREATE INDEX queued_by_task ON queued_messages (task_id, id);
"#,
    r#"
    -- Prompt templates of the user, and changes to the built-in examples
    -- (untouched built-ins have no row).
    CREATE TABLE prompt_templates (
        id       INTEGER PRIMARY KEY,
        -- NULL: every project.
        project  TEXT,
        name     TEXT NOT NULL,
        text     TEXT NOT NULL,
        -- The built-in example this row edits (or deletes).
        builtin  TEXT,
        deleted  INTEGER NOT NULL DEFAULT 0
    );
"#,
    r#"
    -- Last reading of each account's plan usage (JSON), kept to show it at once.
    CREATE TABLE plan_usage (
        account_id    TEXT PRIMARY KEY,
        -- NULL until a reading works; kept when a later attempt fails.
        usage         TEXT,
        attempted_at  INTEGER NOT NULL,
        -- Why the last attempt gave nothing ('unsupported', 'unavailable', 'spends_tokens').
        problem       TEXT
    );
"#,
];

mod queue;
mod templates;

pub struct SqliteStore {
    conn: Mutex<Connection>,
    /// The database file; `None` in memory.
    path: Option<PathBuf>,
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
        let mut store = Self::from_connection(Connection::open(path).map_err(db_error)?)?;
        store.path = Some(path.to_owned());
        // Readers (the memory server) don't block the app while it writes.
        store.with(|c| c.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(())))?;
        Ok(store)
    }

    pub fn in_memory() -> PortResult<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(db_error)?)
    }

    fn from_connection(mut conn: Connection) -> PortResult<Self> {
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(db_error)?;
        // The memory MCP server opens the same file from another process.
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(db_error)?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: None,
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
        return Err(PortError::new(tr!(
            "la base de datos es de una versión más nueva de Hedy",
            "the database comes from a newer version of Hedy"
        )));
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
    let phases = row
        .get::<_, Option<String>>(5)?
        .map(|p| serde_json::from_str(&p))
        .transpose()
        .map_err(invalid(5))?
        .unwrap_or_default();
    let memory_off = row
        .get::<_, Option<String>>(6)?
        .map(|p| serde_json::from_str(&p))
        .transpose()
        .map_err(invalid(6))?
        .unwrap_or_default();
    Ok(Project {
        client: row.get(7)?,
        path: PathBuf::from(row.get::<_, String>(0)?),
        name: row.get(1)?,
        account,
        protected: row.get(3)?,
        color,
        phases,
        memory_off,
    })
}

impl ProjectRepository for SqliteStore {
    fn list_projects(&self) -> PortResult<Vec<Project>> {
        self.with(|c| {
            c.prepare(
                "SELECT path, name, account_id, protected, color, phases, memory_off, client FROM projects ORDER BY name",
            )?
            .query_map([], project_from_row)?
            .collect()
        })
    }

    fn save_project(&self, project: &Project) -> PortResult<()> {
        let phases = (!project.phases.is_empty())
            .then(|| serde_json::to_string(&project.phases))
            .transpose()
            .map_err(json_error)?;
        let memory_off = (!project.memory_off.is_empty())
            .then(|| serde_json::to_string(&project.memory_off))
            .transpose()
            .map_err(json_error)?;
        self.with(|c| {
            c.execute(
                "INSERT INTO projects (path, name, account_id, protected, color, phases, memory_off, client)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (path) DO UPDATE SET
                    name = excluded.name, account_id = excluded.account_id,
                    protected = excluded.protected, color = excluded.color,
                    phases = excluded.phases, memory_off = excluded.memory_off,
                    client = excluded.client",
                params![
                    project.path.to_string_lossy(),
                    project.name,
                    project.account.as_ref().map(AccountId::as_str),
                    project.protected,
                    project.color.map(|c| c.to_string()),
                    phases,
                    memory_off,
                    project.client,
                ],
            )
            .map(drop)
        })
    }
    fn delete_project(&self, path: &Path) -> PortResult<()> {
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

    fn recent_tasks(&self, since: i64) -> PortResult<Vec<Task>> {
        let rows: Vec<String> = self.with(|c| {
            c.prepare(
                "SELECT t.data FROM tasks t
                 WHERE json_extract(t.data, '$.status') IN ('running', 'waitingApproval')
                    OR t.created_at >= ?1
                    OR EXISTS (SELECT 1 FROM task_events e WHERE e.task_id = t.id AND e.at >= ?1)
                 ORDER BY t.created_at DESC, t.id DESC",
            )?
            .query_map([since], |r| r.get(0))?
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
        Ok(self.timed_events(id)?.into_iter().map(|(_, e)| e).collect())
    }

    fn timed_events(&self, id: &TaskId) -> PortResult<Vec<(i64, AgentEvent)>> {
        let rows: Vec<(i64, String)> = self.with(|c| {
            c.prepare("SELECT at, event FROM task_events WHERE task_id = ?1 ORDER BY seq")?
                .query_map([id.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })?;
        rows.into_iter()
            .map(|(at, d)| Ok((at, serde_json::from_str(&d).map_err(json_error)?)))
            .collect()
    }

    fn waiting_tasks(&self) -> PortResult<Vec<Task>> {
        // The wait lives in the task's JSON: no column, no migration.
        let rows: Vec<String> = self.with(|c| {
            c.prepare(
                "SELECT data FROM tasks WHERE json_extract(data, '$.wait') IS NOT NULL
                 ORDER BY created_at DESC, id DESC",
            )?
            .query_map([], |r| r.get(0))?
            .collect()
        })?;
        rows.iter()
            .map(|d| serde_json::from_str(d).map_err(json_error))
            .collect()
    }

    fn conversations(&self) -> PortResult<Vec<Task>> {
        // The flag lives in the task's JSON, like the wait: no migration.
        let rows: Vec<String> = self.with(|c| {
            c.prepare(
                "SELECT data FROM tasks WHERE json_extract(data, '$.conversation') = 1
                 ORDER BY created_at DESC, id DESC",
            )?
            .query_map([], |r| r.get(0))?
            .collect()
        })?;
        rows.iter()
            .map(|d| serde_json::from_str(d).map_err(json_error))
            .collect()
    }

    fn delete_task(&self, id: &TaskId) -> PortResult<()> {
        // Events and queued messages go with it (ON DELETE CASCADE).
        self.with(|c| {
            c.execute("DELETE FROM tasks WHERE id = ?1", [id.as_str()])
                .map(drop)
        })
    }
}

const USAGE_SUMS: &str = "COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0),
     COALESCE(SUM(cache_read_tokens), 0), COALESCE(SUM(cache_write_tokens), 0), SUM(cost_usd)";

/// Five usage columns starting at `first`.
fn usage_from_row(r: &Row, first: usize) -> rusqlite::Result<UsageSample> {
    Ok(UsageSample {
        input_tokens: r.get::<_, i64>(first)? as u64,
        output_tokens: r.get::<_, i64>(first + 1)? as u64,
        cache_read_tokens: r.get::<_, i64>(first + 2)? as u64,
        cache_write_tokens: r.get::<_, i64>(first + 3)? as u64,
        cost_usd: r.get(first + 4)?,
    })
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
                &format!("SELECT {USAGE_SUMS} FROM usage WHERE account_id = ?1 AND at >= ?2"),
                params![account.as_str(), since],
                |r| usage_from_row(r, 0),
            )
        })
    }

    fn usage_by_project(&self, since: i64) -> PortResult<Vec<(PathBuf, UsageSample)>> {
        self.with(|c| {
            c.prepare(&format!(
                "SELECT project, {USAGE_SUMS} FROM usage WHERE at >= ?1 GROUP BY project ORDER BY project"
            ))?
            .query_map([since], |r| Ok((PathBuf::from(r.get::<_, String>(0)?), usage_from_row(r, 1)?)))?
            .collect()
        })
    }

    fn usage_points(&self, project: &Path, since: i64) -> PortResult<Vec<(i64, UsageSample)>> {
        self.with(|c| {
            c.prepare(
                "SELECT at, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost_usd
                 FROM usage WHERE project = ?1 AND at >= ?2 ORDER BY at",
            )?
            .query_map(params![project.to_string_lossy(), since], |r| {
                Ok((r.get(0)?, usage_from_row(r, 1)?))
            })?
            .collect()
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

    fn set_plan_usage(&self, account: &AccountId, record: &PlanUsageRecord) -> PortResult<()> {
        let usage = record
            .usage
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| PortError::new(e.to_string()))?;
        self.with(|c| {
            c.execute(
                "INSERT INTO plan_usage (account_id, usage, attempted_at, problem)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (account_id) DO UPDATE SET usage = excluded.usage,
                    attempted_at = excluded.attempted_at, problem = excluded.problem",
                params![
                    account.as_str(),
                    usage,
                    record.attempted_at,
                    record.problem.map(UsageProblem::as_str),
                ],
            )
            .map(drop)
        })
    }

    fn plan_usage(&self, account: &AccountId) -> PortResult<Option<PlanUsageRecord>> {
        self.with(|c| {
            c.query_row(
                "SELECT usage, attempted_at, problem FROM plan_usage WHERE account_id = ?1",
                [account.as_str()],
                |r| {
                    Ok(PlanUsageRecord {
                        // A reading in an older shape is dropped, not an error.
                        usage: r
                            .get::<_, Option<String>>(0)?
                            .and_then(|json| serde_json::from_str::<PlanUsage>(&json).ok()),
                        attempted_at: r.get(1)?,
                        problem: r
                            .get::<_, Option<String>>(2)?
                            .as_deref()
                            .and_then(UsageProblem::parse),
                    })
                },
            )
            .optional()
        })
    }
}

/// Note columns, for queries that name the notes table `n`.
const NOTE_COLUMNS: &str = "n.id, n.project, n.text, n.category, n.author, n.status, \
     n.created_at, n.uses, n.last_used, n.topic, n.updated_at";

fn note_from_row(row: &Row) -> rusqlite::Result<Note> {
    Ok(Note {
        id: row.get(0)?,
        project: row.get::<_, Option<String>>(1)?.map(PathBuf::from),
        text: row.get(2)?,
        category: row.get::<_, String>(3)?.parse().map_err(invalid(3))?,
        author: row.get(4)?,
        status: if row.get::<_, String>(5)? == "proposed" {
            NoteStatus::Proposed
        } else {
            NoteStatus::Approved
        },
        created_at: row.get(6)?,
        uses: row.get(7)?,
        last_used: row.get(8)?,
        topic: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn status_str(status: NoteStatus) -> &'static str {
    match status {
        NoteStatus::Proposed => "proposed",
        NoteStatus::Approved => "approved",
    }
}

/// `WHERE` clause and parameter for a memory scope.
fn scope(project: Option<&Path>, with_global: bool) -> (&'static str, Option<String>) {
    let path = project.map(|p| p.to_string_lossy().into_owned());
    let clause = match (path.is_some(), with_global) {
        (true, true) => "(n.project = ?1 OR n.project IS NULL)",
        (true, false) => "n.project = ?1",
        // Only global notes; `?1` is NULL here but must appear in the query.
        (false, _) => "(n.project IS NULL AND ?1 IS NULL)",
    };
    (clause, path)
}

/// Free text → an FTS5 query matching any word (prefixes included).
fn fts_query(text: &str) -> Option<String> {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2)
        .map(|w| format!("\"{}\"*", w.to_lowercase()))
        .collect();
    (!words.is_empty()).then(|| words.join(" OR "))
}

impl MemoryRepository for SqliteStore {
    fn add_note(&self, note: &NewNote) -> PortResult<Note> {
        let id = self.with(|c| {
            c.execute(
                "INSERT INTO notes (project, text, category, author, status, created_at, topic)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    note.project
                        .as_ref()
                        .map(|p| p.to_string_lossy().into_owned()),
                    note.text,
                    note.category.as_str(),
                    note.author,
                    status_str(note.status),
                    note.created_at,
                    note.topic,
                ],
            )?;
            Ok(c.last_insert_rowid())
        })?;
        self.note(id)?
            .ok_or_else(|| PortError::new(tr!("la nota no se guardó", "the note was not saved")))
    }

    fn note(&self, id: i64) -> PortResult<Option<Note>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {NOTE_COLUMNS} FROM notes n WHERE n.id = ?1"),
                [id],
                note_from_row,
            )
            .optional()
        })
    }

    fn update_note(&self, id: i64, text: &str, category: Category) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "UPDATE notes SET text = ?2, category = ?3 WHERE id = ?1",
                params![id, text, category.as_str()],
            )
            .map(drop)
        })
    }

    fn revise_note(&self, id: i64, r: &NoteRevision) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "UPDATE notes SET text = ?2, category = ?3, author = ?4, topic = ?5, updated_at = ?6
                 WHERE id = ?1",
                params![
                    id,
                    r.text,
                    r.category.as_str(),
                    r.author,
                    r.topic,
                    r.updated_at
                ],
            )
            .map(drop)
        })
    }

    fn set_note_status(&self, id: i64, status: NoteStatus) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "UPDATE notes SET status = ?2 WHERE id = ?1",
                params![id, status_str(status)],
            )
            .map(drop)
        })
    }

    fn delete_note(&self, id: i64) -> PortResult<()> {
        self.with(|c| c.execute("DELETE FROM notes WHERE id = ?1", [id]).map(drop))
    }

    fn notes(&self, project: Option<&Path>, with_global: bool) -> PortResult<Vec<Note>> {
        let (clause, path) = scope(project, with_global);
        self.with(|c| {
            c.prepare(&format!(
                "SELECT {NOTE_COLUMNS} FROM notes n WHERE {clause} ORDER BY n.created_at DESC, n.id DESC"
            ))?
            .query_map([path], note_from_row)?
            .collect()
        })
    }

    fn search_notes(
        &self,
        project: Option<&Path>,
        with_global: bool,
        query: &str,
    ) -> PortResult<Vec<Note>> {
        let Some(fts) = fts_query(query) else {
            return self.notes(project, with_global);
        };
        let (clause, path) = scope(project, with_global);
        self.with(|c| {
            c.prepare(&format!(
                "SELECT {NOTE_COLUMNS} FROM notes_fts f JOIN notes n ON n.id = f.rowid
                 WHERE notes_fts MATCH ?2 AND {clause} ORDER BY bm25(notes_fts)"
            ))?
            .query_map(params![path, fts], note_from_row)?
            .collect()
        })
    }

    fn record_uses(&self, ids: &[i64], task: Option<&TaskId>, at: i64) -> PortResult<()> {
        let mut conn = self.conn.lock().expect("sqlite mutex poisoned");
        let tx = conn.transaction().map_err(db_error)?;
        for id in ids {
            tx.execute(
                "UPDATE notes SET uses = uses + 1, last_used = ?2 WHERE id = ?1",
                params![id, at],
            )
            .map_err(db_error)?;
            tx.execute(
                "INSERT INTO note_uses (note_id, task_id, at) VALUES (?1, ?2, ?3)",
                params![id, task.map(TaskId::as_str), at],
            )
            .map_err(db_error)?;
        }
        tx.commit().map_err(db_error)
    }

    fn notes_used_by(&self, task: &TaskId) -> PortResult<Vec<Note>> {
        self.with(|c| {
            c.prepare(&format!(
                "SELECT DISTINCT {NOTE_COLUMNS} FROM note_uses u JOIN notes n ON n.id = u.note_id
                 WHERE u.task_id = ?1 ORDER BY n.id"
            ))?
            .query_map([task.as_str()], note_from_row)?
            .collect()
        })
    }

    fn dismiss_pair(&self, a: i64, b: i64) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO note_dismissed_pairs (a, b) VALUES (?1, ?2)",
                params![a.min(b), a.max(b)],
            )
            .map(drop)
        })
    }

    fn dismissed_pairs(&self) -> PortResult<Vec<(i64, i64)>> {
        self.with(|c| {
            c.prepare("SELECT a, b FROM note_dismissed_pairs")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
    }
}

const SKILL_COLUMNS: &str = "id, project, name, description, body, status, author, \
     created_at, updated_at, source_task";

fn skill_from_row(row: &Row) -> rusqlite::Result<Skill> {
    Ok(Skill {
        id: row.get(0)?,
        project: row.get::<_, Option<String>>(1)?.map(PathBuf::from),
        name: row.get(2)?,
        description: row.get(3)?,
        body: row.get(4)?,
        status: if row.get::<_, String>(5)? == "proposed" {
            SkillStatus::Proposed
        } else {
            SkillStatus::Approved
        },
        author: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        source_task: row.get::<_, Option<String>>(9)?.map(TaskId::new),
    })
}

fn skill_status_str(status: SkillStatus) -> &'static str {
    match status {
        SkillStatus::Proposed => "proposed",
        SkillStatus::Approved => "approved",
    }
}

fn path_text(path: Option<&PathBuf>) -> Option<String> {
    path.map(|p| p.to_string_lossy().into_owned())
}

impl SkillRepository for SqliteStore {
    fn add_skill(&self, s: &NewSkill) -> PortResult<Skill> {
        let id = self.with(|c| {
            c.execute(
                "INSERT INTO skills (project, name, description, body, status, author, created_at,
                                     source_task)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    path_text(s.project.as_ref()),
                    s.name,
                    s.description,
                    s.body,
                    skill_status_str(s.status),
                    s.author,
                    s.created_at,
                    s.source_task.as_ref().map(TaskId::as_str),
                ],
            )?;
            Ok(c.last_insert_rowid())
        })?;
        self.skill(id)?.ok_or_else(|| {
            PortError::new(tr!("la habilidad no se guardó", "the skill was not saved"))
        })
    }

    fn skill(&self, id: i64) -> PortResult<Option<Skill>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {SKILL_COLUMNS} FROM skills WHERE id = ?1"),
                [id],
                skill_from_row,
            )
            .optional()
        })
    }

    fn revise_skill(&self, id: i64, r: &SkillRevision) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "UPDATE skills SET project = ?2, name = ?3, description = ?4, body = ?5,
                                   author = ?6, updated_at = ?7
                 WHERE id = ?1",
                params![
                    id,
                    path_text(r.project.as_ref()),
                    r.name,
                    r.description,
                    r.body,
                    r.author,
                    r.updated_at
                ],
            )
            .map(drop)
        })
    }

    fn set_skill_status(&self, id: i64, status: SkillStatus) -> PortResult<()> {
        self.with(|c| {
            c.execute(
                "UPDATE skills SET status = ?2 WHERE id = ?1",
                params![id, skill_status_str(status)],
            )
            .map(drop)
        })
    }

    fn delete_skill(&self, id: i64) -> PortResult<()> {
        self.with(|c| {
            c.execute("DELETE FROM skills WHERE id = ?1", [id])
                .map(drop)
        })
    }

    fn skills(&self, project: Option<&Path>, with_global: bool) -> PortResult<Vec<Skill>> {
        let (clause, path) = scope(project, with_global);
        let clause = clause.replace("n.project", "project");
        self.with(|c| {
            c.prepare(&format!(
                "SELECT {SKILL_COLUMNS} FROM skills WHERE {clause} ORDER BY name, id"
            ))?
            .query_map([path], skill_from_row)?
            .collect()
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

impl StoreHealth for SqliteStore {
    fn data_dir(&self) -> Option<PathBuf> {
        self.path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_owned)
    }

    fn integrity_problems(&self) -> PortResult<Vec<String>> {
        let rows: Vec<String> = self.with(|c| {
            let mut stmt = c.prepare("PRAGMA quick_check")?;
            stmt.query_map([], |r| r.get(0))?.collect()
        })?;
        Ok(rows.into_iter().filter(|r| r != "ok").collect())
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
            phases: crate::domain::PhasePlan::from([(
                crate::domain::Phase::Tests,
                crate::domain::Assignment {
                    account: acc.id.clone(),
                    model: Some("kimi-k3".into()),
                },
            )]),
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
    fn recent_tasks_across_projects() {
        use crate::domain::{TaskMode, TaskStatus};
        let store = SqliteStore::in_memory().unwrap();
        let task = |id: &str, project: &str, at: i64, status: TaskStatus| Task {
            images: Vec::new(),
            id: TaskId::new(id),
            project: project.into(),
            mode: TaskMode::Project,
            title: id.into(),
            prompt: id.into(),
            account: AccountId::new("personal").unwrap(),
            model: None,
            workspace: None,
            session_id: None,
            status,
            created_at: at,
            stages: vec![],
            stage: 0,
            plan: None,
            merge: None,
            wait: None,
            conversation: false,
        };
        // Old but still running; old with a recent event; recent; old and done.
        let running = task("t1", "/code/a", 10, TaskStatus::Running);
        let touched = task("t2", "/code/b", 20, TaskStatus::Review);
        let recent = task("t3", "/code/b", 500, TaskStatus::Failed);
        let done = task("t4", "/code/a", 30, TaskStatus::Accepted);
        for t in [&running, &touched, &recent, &done] {
            store.save_task(t).unwrap();
        }
        let event = AgentEvent::Message { text: "ok".into() };
        store.append_event(&touched.id, 450, &event).unwrap();
        store.append_event(&done.id, 40, &event).unwrap();
        let ids: Vec<_> = store
            .recent_tasks(400)
            .unwrap()
            .into_iter()
            .map(|t| t.id.to_string())
            .collect();
        assert_eq!(ids, vec!["t3", "t2", "t1"]);
    }

    #[test]
    fn waits_and_merges_live_in_the_task_and_old_rows_still_load() {
        use crate::domain::{LimitWait, MergeRecord, TaskMode, TaskStatus};
        let store = SqliteStore::in_memory().unwrap();
        let mut task = Task {
            images: Vec::new(),
            id: TaskId::new("t1"),
            project: "/code/moira".into(),
            mode: TaskMode::Project,
            title: "Recordatorios".into(),
            prompt: "Recordatorios".into(),
            account: AccountId::new("personal").unwrap(),
            model: None,
            workspace: None,
            session_id: None,
            status: TaskStatus::Failed,
            created_at: 10,
            stages: vec![],
            stage: 0,
            plan: None,
            merge: None,
            wait: None,
            conversation: false,
        };
        store.save_task(&task).unwrap();
        assert!(store.waiting_tasks().unwrap().is_empty());

        task.wait = Some(LimitWait {
            account: task.account.clone(),
            until: Some(1_800_000_000),
            since: 100,
            ready: false,
        });
        task.merge = Some(MergeRecord {
            before: "a".into(),
            after: "b".into(),
            tip: "c".into(),
            at: 5,
        });
        store.save_task(&task).unwrap();
        assert_eq!(store.waiting_tasks().unwrap(), vec![task.clone()]);
        assert_eq!(store.task(&task.id).unwrap(), Some(task.clone()));

        // A row written before these fields existed.
        store
            .with(|c| {
                c.execute(
                    "INSERT INTO tasks (id, project, created_at, data) VALUES ('t0', '/code/moira', 1, ?1)",
                    [r#"{"id":"t0","project":"/code/moira","mode":"project","title":"x","prompt":"x","account":"personal","model":null,"workspace":null,"sessionId":null,"status":"review","createdAt":1}"#],
                )
                .map(drop)
            })
            .unwrap();
        let old = store.task(&TaskId::new("t0")).unwrap().unwrap();
        assert_eq!((old.merge, old.wait), (None, None));
        assert_eq!(store.waiting_tasks().unwrap().len(), 1);
    }

    #[test]
    fn conversations_are_listed_apart_and_deleted_with_their_events() {
        let store = SqliteStore::in_memory().unwrap();
        let task = |id: &str, conversation: bool| Task {
            images: Vec::new(),
            id: TaskId::new(id),
            project: format!("/data/chats/{id}").into(),
            mode: crate::domain::TaskMode::Ask,
            title: "¿Qué es un monad?".into(),
            prompt: "¿Qué es un monad?".into(),
            account: AccountId::new("personal").unwrap(),
            model: None,
            workspace: None,
            session_id: None,
            status: crate::domain::TaskStatus::Review,
            created_at: 10,
            stages: vec![],
            stage: 0,
            plan: None,
            merge: None,
            wait: None,
            conversation,
        };
        let (chat, other) = (task("c1", true), task("t1", false));
        store.save_task(&chat).unwrap();
        store.save_task(&other).unwrap();
        store
            .append_event(
                &chat.id,
                11,
                &AgentEvent::Error {
                    message: "x".into(),
                },
            )
            .unwrap();
        assert_eq!(store.conversations().unwrap(), vec![chat.clone()]);

        store.delete_task(&chat.id).unwrap();
        assert!(store.conversations().unwrap().is_empty());
        assert_eq!(store.task(&chat.id).unwrap(), None);
        assert!(store.events(&chat.id).unwrap().is_empty());
        assert_eq!(store.task(&other.id).unwrap(), Some(other));
    }

    #[test]
    fn tasks_events_and_usage_round_trip() {
        use crate::domain::{TaskMode, TaskStatus, Workspace};
        let store = SqliteStore::in_memory().unwrap();
        let task = |id: &str, at: i64| Task {
            images: Vec::new(),
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
            stages: vec![],
            stage: 0,
            plan: None,
            merge: None,
            wait: None,
            conversation: false,
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
                images: Vec::new(),
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

        assert_eq!(store.plan_usage(&account).unwrap(), None);
        let usage = crate::domain::plan_usage::resolve_windows(
            crate::domain::plan_usage::parse_usage_text(
                "Current session: 34% used · resets in 1 hr",
            ),
            10,
            crate::domain::UsageSource::ClaudeUsage,
            |_| None,
        );
        let mut record = PlanUsageRecord {
            usage: Some(usage),
            attempted_at: 10,
            problem: None,
        };
        store.set_plan_usage(&account, &record).unwrap();
        assert_eq!(store.plan_usage(&account).unwrap(), Some(record.clone()));
        record.attempted_at = 20;
        record.problem = Some(UsageProblem::Unavailable);
        store.set_plan_usage(&account, &record).unwrap();
        assert_eq!(store.plan_usage(&account).unwrap(), Some(record));
    }

    #[test]
    fn memory_notes_search_and_uses() {
        let store = SqliteStore::in_memory().unwrap();
        let add = |project: Option<&str>, text: &str, status: NoteStatus| {
            store
                .add_note(&NewNote {
                    project: project.map(PathBuf::from),
                    text: text.into(),
                    category: Category::Decision,
                    author: "user".into(),
                    status,
                    created_at: 10,
                    topic: None,
                })
                .unwrap()
        };
        let price = add(
            Some("/code/moira"),
            "Los precios se guardan en USD",
            NoteStatus::Approved,
        );
        let remind = add(
            Some("/code/moira"),
            "Los recordatorios se envían por WhatsApp",
            NoteStatus::Proposed,
        );
        let global = add(
            None,
            "Prefiere respuestas cortas y en español",
            NoteStatus::Approved,
        );
        add(
            Some("/code/otro"),
            "Nota de otro proyecto",
            NoteStatus::Approved,
        );

        let ids = |notes: Vec<Note>| notes.iter().map(|n| n.id).collect::<Vec<_>>();
        let moira = Some(Path::new("/code/moira"));
        assert_eq!(ids(store.notes(moira, true).unwrap()).len(), 3);
        assert_eq!(ids(store.notes(moira, false).unwrap()).len(), 2);
        assert_eq!(ids(store.notes(None, false).unwrap()), vec![global.id]);

        // Accents and prefixes match: "recordatorio" finds "recordatorios".
        assert_eq!(
            ids(store
                .search_notes(moira, true, "recordatorio whatsapp")
                .unwrap()),
            vec![remind.id]
        );
        assert_eq!(
            ids(store.search_notes(moira, true, "espanol").unwrap()),
            vec![global.id]
        );
        assert!(
            store
                .search_notes(moira, false, "espanol")
                .unwrap()
                .is_empty()
        );

        store
            .update_note(
                price.id,
                "Los precios se guardan en USD y Bs",
                Category::Convention,
            )
            .unwrap();
        assert_eq!(
            ids(store.search_notes(moira, true, "Bs").unwrap()),
            vec![price.id]
        );
        store
            .set_note_status(remind.id, NoteStatus::Approved)
            .unwrap();
        assert_eq!(
            store.note(remind.id).unwrap().unwrap().status,
            NoteStatus::Approved
        );

        let task = TaskId::new("t1");
        store
            .record_uses(&[price.id, global.id], Some(&task), 99)
            .unwrap();
        store.record_uses(&[price.id], Some(&task), 100).unwrap();
        let used = store.notes_used_by(&task).unwrap();
        assert_eq!(ids(used.clone()), vec![price.id, global.id]);
        assert_eq!((used[0].uses, used[0].last_used), (2, Some(100)));

        store.dismiss_pair(5, 2).unwrap();
        store.dismiss_pair(2, 5).unwrap();
        assert_eq!(store.dismissed_pairs().unwrap(), vec![(2, 5)]);

        store.delete_note(price.id).unwrap();
        assert!(store.search_notes(moira, true, "USD").unwrap().is_empty());
        assert!(
            store
                .notes_used_by(&task)
                .unwrap()
                .iter()
                .all(|n| n.id != price.id)
        );
    }

    #[test]
    fn topics_are_searched_and_notes_revised_in_place() {
        let store = SqliteStore::in_memory().unwrap();
        let moira = Path::new("/code/moira");
        let note = store
            .add_note(&NewNote {
                project: Some(moira.into()),
                text: "Los pagos van por Stripe".into(),
                category: Category::Decision,
                author: "Claude".into(),
                status: NoteStatus::Approved,
                created_at: 10,
                topic: Some("arquitectura/cobros".into()),
            })
            .unwrap();
        assert_eq!(note.topic.as_deref(), Some("arquitectura/cobros"));
        assert_eq!(note.updated_at, None);
        // The topic is indexed too.
        let found = store.search_notes(Some(moira), false, "cobros").unwrap();
        assert_eq!(found.iter().map(|n| n.id).collect::<Vec<_>>(), [note.id]);

        store.record_uses(&[note.id], None, 20).unwrap();
        store
            .revise_note(
                note.id,
                &NoteRevision {
                    text: "Los pagos van por Mercado Pago".into(),
                    category: Category::Rule,
                    author: "Codex".into(),
                    topic: Some("arquitectura/pagos".into()),
                    updated_at: 30,
                },
            )
            .unwrap();
        let revised = store.note(note.id).unwrap().unwrap();
        assert_eq!(revised.text, "Los pagos van por Mercado Pago");
        assert_eq!(
            (revised.category, revised.author.as_str()),
            (Category::Rule, "Codex")
        );
        assert_eq!(
            (revised.created_at, revised.uses),
            (10, 1),
            "keeps its history"
        );
        assert_eq!(revised.updated_at, Some(30));
        // The index follows the new text and topic.
        assert!(
            store
                .search_notes(Some(moira), false, "Stripe")
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .search_notes(Some(moira), false, "cobros")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store
                .search_notes(Some(moira), false, "mercado")
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .search_notes(Some(moira), false, "pagos")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn similar_notes_are_found_by_their_key_words() {
        use crate::domain::memory::{key_words, similar};
        let store = SqliteStore::in_memory().unwrap();
        let moira = Path::new("/code/moira");
        let add = |text: &str| {
            store
                .add_note(&NewNote {
                    project: Some(moira.into()),
                    text: text.into(),
                    category: Category::Decision,
                    author: "user".into(),
                    status: NoteStatus::Approved,
                    created_at: 1,
                    topic: None,
                })
                .unwrap()
        };
        let prices = add("Los precios se guardan en USD y se muestran también en Bs");
        add("Los recordatorios se envían por WhatsApp");
        add("Tests con pytest");
        let proposal = "El precio se guarda siempre en dólares USD";
        // What the memory screen does: search by key words (plurals and
        // accents folded), then keep the close ones.
        let query = key_words(proposal).join(" ");
        let found: Vec<Note> = store
            .search_notes(Some(moira), false, &query)
            .unwrap()
            .into_iter()
            .filter(|n| similar(&n.text, proposal))
            .collect();
        assert_eq!(found.iter().map(|n| n.id).collect::<Vec<_>>(), [prices.id]);
    }

    #[test]
    fn the_topic_migration_keeps_existing_notes_searchable() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hedy.db");
        {
            // A database from before topics: migrations up to the 6th.
            let conn = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..6] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", 6).unwrap();
            conn.execute(
                "INSERT INTO notes (project, text, category, author, status, created_at)
                 VALUES ('/code/moira', 'Los recordatorios se envían por WhatsApp', 'decision',
                         'user', 'approved', 5)",
                [],
            )
            .unwrap();
        }
        let store = SqliteStore::open(&path).unwrap();
        let notes = store
            .search_notes(Some(Path::new("/code/moira")), false, "recordatorio")
            .unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!((notes[0].topic.as_ref(), notes[0].updated_at), (None, None));
        store.delete_note(notes[0].id).unwrap();
        assert!(
            store
                .search_notes(Some(Path::new("/code/moira")), false, "whatsapp")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn skills_round_trip_by_scope() {
        let store = SqliteStore::in_memory().unwrap();
        let moira = PathBuf::from("/code/moira");
        let new = |project: Option<&PathBuf>, name: &str, status| NewSkill {
            project: project.cloned(),
            name: name.into(),
            description: "Cuándo usarla".into(),
            body: "1. npm test".into(),
            author: "Claude".into(),
            status,
            created_at: 10,
            source_task: Some(TaskId::new("t-1")),
        };
        let tests = store
            .add_skill(&new(Some(&moira), "run-tests", SkillStatus::Proposed))
            .unwrap();
        assert_eq!(tests.source_task, Some(TaskId::new("t-1")));
        assert_eq!(
            (tests.status, tests.updated_at),
            (SkillStatus::Proposed, None)
        );
        store
            .add_skill(&new(None, "a-global", SkillStatus::Approved))
            .unwrap();
        store
            .add_skill(&new(
                Some(&PathBuf::from("/code/otro")),
                "x",
                SkillStatus::Approved,
            ))
            .unwrap();

        let names = |skills: Vec<Skill>| skills.into_iter().map(|s| s.name).collect::<Vec<_>>();
        assert_eq!(
            names(store.skills(Some(&moira), true).unwrap()),
            ["a-global", "run-tests"]
        );
        assert_eq!(
            names(store.skills(Some(&moira), false).unwrap()),
            ["run-tests"]
        );
        assert_eq!(names(store.skills(None, false).unwrap()), ["a-global"]);

        store
            .set_skill_status(tests.id, SkillStatus::Approved)
            .unwrap();
        store
            .revise_skill(
                tests.id,
                &SkillRevision {
                    project: None,
                    name: "run-all-tests".into(),
                    description: "Antes de entregar".into(),
                    body: "1. cargo test".into(),
                    author: "user".into(),
                    updated_at: 20,
                },
            )
            .unwrap();
        let revised = store.skill(tests.id).unwrap().unwrap();
        assert_eq!(
            (revised.name.as_str(), revised.project, revised.updated_at),
            ("run-all-tests", None, Some(20))
        );
        assert_eq!(
            (revised.created_at, revised.status),
            (10, SkillStatus::Approved)
        );
        store.delete_skill(tests.id).unwrap();
        assert_eq!(store.skill(tests.id).unwrap(), None);
    }

    #[test]
    fn the_skills_migration_keeps_existing_data() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hedy.db");
        let before = MIGRATIONS.len() - 1;
        {
            // A database from before skills.
            let conn = Connection::open(&path).unwrap();
            for sql in &MIGRATIONS[..before] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", before as i64)
                .unwrap();
            conn.execute(
                "INSERT INTO notes (project, text, category, author, status, created_at)
                 VALUES ('/code/moira', 'Precios en USD', 'decision', 'user', 'approved', 5)",
                [],
            )
            .unwrap();
        }
        let store = SqliteStore::open(&path).unwrap();
        assert_eq!(
            store
                .notes(Some(Path::new("/code/moira")), false)
                .unwrap()
                .len(),
            1
        );
        assert!(store.skills(None, true).unwrap().is_empty());
    }

    #[test]
    fn migrations_are_idempotent_on_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested/hedy.db");
        SqliteStore::open(&path).unwrap();
        let store = SqliteStore::open(&path).unwrap();
        assert!(store.list_accounts().unwrap().is_empty());
        assert_eq!(store.data_dir(), Some(tmp.path().join("nested")));
        assert_eq!(store.integrity_problems().unwrap(), Vec::<String>::new());
        assert_eq!(SqliteStore::in_memory().unwrap().data_dir(), None);
    }
}
