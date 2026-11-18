//! Two PCs (fixtures) syncing through one shared server.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::*;
use crate::application::NewTemplate;
use crate::application::ports::{
    MemoryRepository, SecretStore, SyncRemote, SyncRepository, TemplateRepository,
};
use crate::application::testing::{Fixture, SyncServer};
use crate::application::themes::NewTheme;
use crate::domain::theme::{ThemeBase, ThemeOrigin};
use crate::domain::{Category, NoteStatus};

const URL: &str = "git@github.com:tu-usuario/hedy-memory.git";

/// A PC with a Claude account and the project "tienda" at `path`, whose
/// origin is `tu-usuario/tienda`.
fn pc(server: &SyncServer, path: &str) -> (Fixture, PathBuf) {
    let f = Fixture::with_server(server.clone());
    f.add_claude("Personal");
    f.fs.add_git_root(path);
    let project = f.dir(path);
    f.hedy.use_account(&project, "personal", None).unwrap();
    f.source
        .set_remotes(&[("origin", "git@github.com:Tu-Usuario/Tienda.git")]);
    (f, project)
}

fn texts(f: &Fixture, project: Option<&std::path::Path>) -> Vec<String> {
    let mut texts: Vec<String> = f
        .store
        .notes(project, false)
        .unwrap()
        .into_iter()
        .map(|n| n.text)
        .collect();
    texts.sort();
    texts
}

/// Home sets up; office joins with the words.
fn two_pcs() -> (SyncServer, (Fixture, PathBuf), (Fixture, PathBuf)) {
    let server = SyncServer::default();
    let home = pc(&server, "/home/u/code/tienda");
    let office = pc(&server, "/work/tienda");
    for (f, project) in [&home, &office] {
        f.hedy
            .set_memory_home(project, Some(&MemoryHome::Personal))
            .unwrap();
    }
    let words = home.0.hedy.sync_setup(URL, "Casa").unwrap();
    office.0.hedy.sync_join(URL, &words, "Oficina").unwrap();
    (server, home, office)
}

#[test]
fn setup_shows_the_words_and_a_new_pc_joins_with_them() {
    let server = SyncServer::default();
    let (home, _) = pc(&server, "/home/u/code/tienda");
    home.hedy
        .add_note(None, "Responde siempre en español", Category::Preference)
        .unwrap();
    assert!(!home.hedy.sync_is_set_up().unwrap());

    let words = home.hedy.sync_setup(URL, "Casa").unwrap();
    assert_eq!(words.split_whitespace().count(), 12);
    assert!(home.hedy.sync_is_set_up().unwrap());
    assert_eq!(
        home.hedy.sync_setup(URL, "Casa"),
        Err(AppError::SyncAlreadySetUp)
    );
    let paths = server.paths();
    assert!(paths.contains(&"README.md".to_owned()));
    assert!(paths.contains(&layout::epoch(1)));
    // The server never sees the text.
    for path in &paths {
        let bytes = server.file(path).unwrap();
        assert!(
            !String::from_utf8_lossy(&bytes).contains("español"),
            "{path} leaks the note"
        );
    }

    // Another PC: wrong words first, then the right ones.
    let (office, _) = pc(&server, "/work/tienda");
    assert!(office.hedy.sync_join(URL, "árbol nube", "Oficina").is_err());
    assert!(!office.hedy.sync_is_set_up().unwrap());
    let report = office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    assert_eq!(report.received, 1);
    assert_eq!(texts(&office, None), ["Responde siempre en español"]);
    assert!(server.paths().contains(&layout::epoch(2)));

    // Home learns about epoch 2 and both PCs show up.
    home.hedy.sync_now().unwrap();
    let status = home.hedy.sync_status().unwrap().unwrap();
    let names: Vec<(&str, bool)> = status
        .devices
        .iter()
        .map(|d| (d.name.as_str(), d.this_pc))
        .collect();
    assert_eq!(names, [("Casa", true), ("Oficina", false)]);
    assert_eq!(status.url, URL);
}

#[test]
fn setup_and_join_refuse_the_wrong_repo() {
    let server = SyncServer::default();
    let (home, _) = pc(&server, "/a");
    let (office, _) = pc(&server, "/b");
    assert_eq!(
        office.hedy.sync_join(URL, "x", "Oficina"),
        Err(AppError::SyncRepoEmpty)
    );
    home.hedy.sync_setup(URL, "Casa").unwrap();
    assert_eq!(
        office.hedy.sync_setup(URL, "Oficina"),
        Err(AppError::SyncRepoInUse)
    );
    assert_eq!(office.hedy.sync_now(), Err(AppError::SyncNotSetUp));
    assert_eq!(office.hedy.sync_status().unwrap(), None);
}

#[test]
fn personal_project_notes_follow_the_repo_not_the_path() {
    let (_server, (home, home_tienda), (office, office_tienda)) = two_pcs();
    home.hedy
        .add_note(
            Some(&home_tienda),
            "Precios en USD 12.50",
            Category::Convention,
        )
        .unwrap();
    // A project kept local on this PC stays here.
    home.fs.add_git_root("/home/u/code/privado");
    let privado = home.dir("/home/u/code/privado");
    home.hedy.use_account(&privado, "personal", None).unwrap();
    home.hedy
        .set_memory_home(&privado, Some(&MemoryHome::Local))
        .unwrap();
    home.hedy
        .add_note(Some(&privado), "Nota privada", Category::Decision)
        .unwrap();

    let sent = home.hedy.sync_now().unwrap();
    assert_eq!(sent.sent, 1);
    office.hedy.sync_now().unwrap();
    assert_eq!(
        texts(&office, Some(&office_tienda)),
        ["Precios en USD 12.50"]
    );
    assert!(texts(&office, None).is_empty());

    // Nothing new: nothing sent, nothing pushed.
    let pushes = home.sync.pushes().len();
    assert_eq!(home.hedy.sync_now().unwrap(), SyncReport::default());
    assert_eq!(home.sync.pushes().len(), pushes);
}

#[test]
fn a_project_not_on_this_pc_waits_in_the_replica() {
    let (_server, (home, home_tienda), (office, office_tienda)) = two_pcs();
    office
        .hedy
        .set_memory_home(&office_tienda, Some(&MemoryHome::Local))
        .unwrap();
    home.hedy
        .add_note(Some(&home_tienda), "Tests con pytest", Category::Convention)
        .unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    assert!(texts(&office, Some(&office_tienda)).is_empty());

    // Made personal later: the next sync does not bring it again by itself,
    // but its version is in the replica for when the project matches.
    assert_eq!(office.store.sync_items().unwrap().len(), 1);
}

#[test]
fn edits_and_deletions_reach_the_other_pc() {
    let (_server, (home, _), (office, _)) = two_pcs();
    let note = home
        .hedy
        .add_note(None, "Usa tabs", Category::Convention)
        .unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    assert_eq!(texts(&office, None), ["Usa tabs"]);

    home.hedy
        .edit_note(note.id, "Usa espacios", Category::Convention)
        .unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    assert_eq!(texts(&office, None), ["Usa espacios"]);

    let there = office.store.notes(None, false).unwrap()[0].id;
    office.hedy.forget_note(there).unwrap();
    office.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();
    assert!(texts(&home, None).is_empty());
    assert!(home.hedy.sync_conflicts().unwrap().is_empty());
}

#[test]
fn proposals_templates_and_themes_sync_too() {
    let (_server, (home, _), (office, _)) = two_pcs();
    home.store
        .add_note(&crate::application::ports::NewNote {
            project: None,
            text: "Propuesta de Claude".into(),
            category: Category::Decision,
            author: "Claude".into(),
            status: NoteStatus::Proposed,
            created_at: 1,
            topic: None,
        })
        .unwrap();
    home.hedy
        .add_template(NewTemplate {
            project: None,
            name: "Revisión".into(),
            text: "Revisa los cambios".into(),
        })
        .unwrap();
    home.hedy
        .save_theme(NewTheme {
            id: None,
            name: "Noche".into(),
            base: ThemeBase::Dark,
            colors: BTreeMap::from([
                ("bg".to_owned(), "#282a36".to_owned()),
                ("ink".to_owned(), "#f8f8f2".to_owned()),
            ]),
            origin: ThemeOrigin::Vscode,
        })
        .unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();

    let note = &office.store.notes(None, false).unwrap()[0];
    assert_eq!(
        (note.text.as_str(), note.status),
        ("Propuesta de Claude", NoteStatus::Proposed)
    );
    assert!(
        office
            .store
            .template_records()
            .unwrap()
            .iter()
            .any(|t| t.name == "Revisión")
    );
    assert_eq!(office.hedy.list_themes().unwrap()[0].name, "Noche");
    // Applying them did not make them look changed on the office PC.
    assert_eq!(office.hedy.sync_now().unwrap().sent, 0);
}

#[test]
fn edits_made_apart_keep_the_newer_and_offer_the_other() {
    let (_server, (home, _), (office, _)) = two_pcs();
    let note = home
        .hedy
        .add_note(None, "Precios con dos decimales", Category::Convention)
        .unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    let there = office.store.notes(None, false).unwrap()[0].id;

    // Both edit before syncing; the office edits later.
    home.hedy
        .edit_note(note.id, "Precios sin decimales", Category::Convention)
        .unwrap();
    office.clock.advance(60);
    office
        .hedy
        .edit_note(
            there,
            "Precios con decimales si no son cero",
            Category::Convention,
        )
        .unwrap();
    home.hedy.sync_now().unwrap();
    let report = office.hedy.sync_now().unwrap();
    assert_eq!(report.conflicts, 1);
    home.hedy.sync_now().unwrap();

    // Same winner on both PCs: the newer edit.
    for f in [&home, &office] {
        assert_eq!(texts(f, None), ["Precios con decimales si no son cero"]);
        let conflicts = f.hedy.sync_conflicts().unwrap();
        assert_eq!(conflicts.len(), 1);
        assert!(
            conflicts[0]
                .other
                .body
                .as_deref()
                .unwrap()
                .contains("sin decimales")
        );
    }

    // The user picks the other one at home; it reaches the office and the
    // office's conflict goes away.
    let id = home.hedy.sync_conflicts().unwrap()[0].id;
    home.hedy.resolve_sync_conflict(id, true).unwrap();
    assert_eq!(texts(&home, None), ["Precios sin decimales"]);
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    assert_eq!(texts(&office, None), ["Precios sin decimales"]);
    assert!(office.hedy.sync_conflicts().unwrap().is_empty());
    assert!(home.hedy.sync_conflicts().unwrap().is_empty());
    assert_eq!(
        home.hedy.resolve_sync_conflict(id, false),
        Err(AppError::SyncConflictNotFound(id))
    );
}

#[test]
fn a_pc_left_out_of_the_newest_epoch_stops_syncing() {
    let (server, (home, _), (office, _)) = two_pcs();
    home.hedy.sync_now().unwrap();
    // Home writes epoch 3 without the office (what "Quitar PC" will do).
    let keys: Keys = serde_json::from_str(&home.secrets.sync_keys().unwrap().unwrap()).unwrap();
    let crypto = crate::adapters::VaultCrypto;
    use crate::application::ports::SyncCrypto;
    let epoch2 = String::from_utf8(server.file(&layout::epoch(2)).unwrap()).unwrap();
    let (_, members) = crypto.epoch_members(&epoch2).unwrap();
    let home_id = home.hedy.sync_status().unwrap().unwrap().device;
    let kept: Vec<(String, String)> = members
        .into_iter()
        .filter(|(id, _)| id == RECOVERY_MEMBER || id == home_id.as_str())
        .collect();
    let (file, _) = crypto
        .new_epoch(3, &kept, home_id.as_str(), &keys.device)
        .unwrap();
    home.sync
        .write(Path::new("/"), &layout::epoch(3), file.as_bytes())
        .unwrap();
    home.sync.push(Path::new("/"), "remove office").unwrap();

    assert_eq!(office.hedy.sync_now(), Err(AppError::SyncRemoved));
}

#[test]
fn two_hedy_folders_sync_through_a_real_git_repo() {
    use std::sync::Arc;

    use crate::adapters::{GitSyncRemote, SqliteStore};

    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("hedy-memory.git");
    std::fs::create_dir_all(&bare).unwrap();
    let init = std::process::Command::new("git")
        .args(["init", "--quiet", "--bare", "--initial-branch=main"])
        .current_dir(&bare)
        .status()
        .unwrap();
    assert!(init.success());
    let url = bare.to_string_lossy().into_owned();

    // Each PC: its own data folder (SQLite) and clone of the repo.
    let pc = |name: &str, path: &str| {
        let f = Fixture::new();
        let data = tmp.path().join(name);
        std::fs::create_dir_all(&data).unwrap();
        let store = Arc::new(SqliteStore::open(&data.join("hedy.db")).unwrap());
        let hedy = f.hedy_on(store, Arc::new(GitSyncRemote));
        f.fs.add_git_root(path);
        f.source
            .set_remotes(&[("origin", "https://github.com/tu-usuario/tienda")]);
        let project = f.dir(path);
        hedy.add_account(crate::application::NewAccount {
            name: "Personal".into(),
            provider: crate::domain::Provider::Claude,
            api_key: false,
            default_folder: false,
            color: None,
        })
        .unwrap();
        hedy.use_account(&project, "personal", None).unwrap();
        hedy.set_memory_home(&project, Some(&MemoryHome::Personal))
            .unwrap();
        (f, hedy, project)
    };
    let (_home_f, home, home_tienda) = pc("home", "/home/u/tienda");
    let (_office_f, office, office_tienda) = pc("office", "/work/tienda");

    home.add_note(
        Some(&home_tienda),
        "Precios en USD 12.50",
        Category::Convention,
    )
    .unwrap();
    let words = home.sync_setup(&url, "Casa").unwrap();
    office.sync_join(&url, &words, "Oficina").unwrap();
    let notes = office.memory(&office_tienda).unwrap();
    assert!(
        notes.notes.iter().any(|n| n.text == "Precios en USD 12.50"),
        "{notes:?}"
    );

    office
        .add_note(None, "Responde en español", Category::Preference)
        .unwrap();
    office.sync_now().unwrap();
    home.sync_now().unwrap();
    let status = home.sync_status().unwrap().unwrap();
    assert_eq!(status.devices.len(), 2);
    assert!(
        home.memory(&home_tienda)
            .unwrap()
            .notes
            .iter()
            .any(|n| n.text == "Responde en español")
    );
}
