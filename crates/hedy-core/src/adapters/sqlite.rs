use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::application::ports::{
    AccountRepository, MemoryRepository, NewNote, PortError, PortResult, ProjectRepository,
    RateLimit, SettingsRepository, TaskRepository, UsageRecord, UsageRepository,
};
use crate::domain::{
    Account, AccountId, AgentEvent, Category, Color, ConfigFolder, Credentials, Note, NoteStatus,
    Project, Task, TaskId, UsageSample,
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
        let store = Self::from_connection(Connection::open(path).map_err(db_error)?)?;
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
                "SELECT path, name, account_id, protected, color, phases, memory_off FROM projects ORDER BY name",
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
                "INSERT INTO projects (path, name, account_id, protected, color, phases, memory_off)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (path) DO UPDATE SET
                    name = excluded.name, account_id = excluded.account_id,
                    protected = excluded.protected, color = excluded.color,
                    phases = excluded.phases, memory_off = excluded.memory_off",
                params![
                    project.path.to_string_lossy(),
                    project.name,
                    project.account.as_ref().map(AccountId::as_str),
                    project.protected,
                    project.color.map(|c| c.to_string()),
                    phases,
                    memory_off,
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

const NOTE_COLUMNS: &str =
    "id, project, text, category, author, status, created_at, uses, last_used";

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
                "INSERT INTO notes (project, text, category, author, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    note.project
                        .as_ref()
                        .map(|p| p.to_string_lossy().into_owned()),
                    note.text,
                    note.category.as_str(),
                    note.author,
                    status_str(note.status),
                    note.created_at,
                ],
            )?;
            Ok(c.last_insert_rowid())
        })?;
        self.note(id)?
            .ok_or_else(|| PortError::new("la nota no se guardó"))
    }

    fn note(&self, id: i64) -> PortResult<Option<Note>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id = ?1"),
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
        let columns = NOTE_COLUMNS
            .split(", ")
            .map(|c| format!("n.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.with(|c| {
            c.prepare(&format!(
                "SELECT {columns} FROM notes_fts f JOIN notes n ON n.id = f.rowid
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
        let columns = NOTE_COLUMNS
            .split(", ")
            .map(|c| format!("n.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.with(|c| {
            c.prepare(&format!(
                "SELECT DISTINCT {columns} FROM note_uses u JOIN notes n ON n.id = u.note_id
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
            stages: vec![],
            stage: 0,
            plan: None,
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
    fn migrations_are_idempotent_on_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested/hedy.db");
        SqliteStore::open(&path).unwrap();
        let store = SqliteStore::open(&path).unwrap();
        assert!(store.list_accounts().unwrap().is_empty());
    }
}
