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

/// A small deterministic random source for the simulation.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) % below as u64) as usize
    }
}

/// Three PCs add, edit and delete notes and sync in random order; once all
/// of them sync, they hold the same notes and have nothing left to send.
#[test]
fn random_edits_on_three_pcs_converge() {
    for seed in 1..=12u64 {
        simulate(seed, false);
    }
}

/// The same with SQLite, which reuses the id of a deleted last row.
#[test]
fn random_edits_converge_with_sqlite() {
    for seed in 1..=6u64 {
        simulate(seed, true);
    }
}

fn simulate(seed: u64, sqlite: bool) {
    use std::sync::Arc;

    use crate::adapters::SqliteStore;
    use crate::application::testing::FakeSyncRemote;

    let tmp = tempfile::tempdir().unwrap();
    let server = SyncServer::default();
    // (fixture, the Hedy under test, its store, the project)
    let mut pcs = Vec::new();
    for (i, path) in ["/a/tienda", "/b/tienda", "/c/tienda"].iter().enumerate() {
        let (f, project) = pc(&server, path);
        let (hedy, store): (Hedy, Arc<dyn crate::application::ports::Store>) = if sqlite {
            let dir = tmp.path().join(format!("pc{i}"));
            std::fs::create_dir_all(&dir).unwrap();
            let store = Arc::new(SqliteStore::open(&dir.join("hedy.db")).unwrap());
            let hedy = f.hedy_on(store.clone(), Arc::new(FakeSyncRemote::new(server.clone())));
            hedy.add_account(crate::application::NewAccount {
                name: "Personal".into(),
                provider: crate::domain::Provider::Claude,
                api_key: false,
                default_folder: false,
                color: None,
            })
            .unwrap();
            hedy.use_account(&project, "personal", None).unwrap();
            (hedy, store)
        } else {
            (f.hedy.clone(), f.store.clone())
        };
        hedy.set_memory_home(&project, Some(&MemoryHome::Personal))
            .unwrap();
        pcs.push((f, hedy, store, project));
    }
    let words = pcs[0].1.sync_setup(URL, "pc0").unwrap();
    for (i, (_, hedy, _, _)) in pcs.iter().enumerate().skip(1) {
        hedy.sync_join(URL, &words, &format!("pc{i}")).unwrap();
    }

    let texts = |store: &Arc<dyn crate::application::ports::Store>, project: Option<&Path>| {
        let mut texts: Vec<String> = store
            .notes(project, false)
            .unwrap()
            .into_iter()
            .map(|n| n.text)
            .collect();
        texts.sort();
        texts
    };
    let mut rng = Lcg(seed);
    let mut written = 0;
    for _ in 0..150 {
        let (f, hedy, store, project) = &pcs[rng.next(pcs.len())];
        f.clock.advance(rng.next(5) as i64);
        let scope = if rng.next(2) == 0 {
            None
        } else {
            Some(project.as_path())
        };
        let notes = store.notes(scope, false).unwrap();
        match rng.next(6) {
            0 | 1 => {
                written += 1;
                hedy.add_note(scope, &format!("nota {seed}-{written}"), Category::Decision)
                    .unwrap();
            }
            2 if !notes.is_empty() => {
                written += 1;
                let note = &notes[rng.next(notes.len())];
                hedy.edit_note(
                    note.id,
                    &format!("editada {seed}-{written}"),
                    Category::Decision,
                )
                .unwrap();
            }
            3 if !notes.is_empty() => {
                // The newest note: SQLite gives its id to the next one.
                hedy.forget_note(notes[0].id).unwrap();
            }
            _ => {
                hedy.sync_now().unwrap();
            }
        }
    }

    // Everyone syncs twice, in turn: the second round settles what the first
    // one brought.
    for _ in 0..2 {
        for (_, hedy, _, _) in &pcs {
            hedy.sync_now().unwrap();
        }
    }
    let state = |(_, _, store, project): &(
        Fixture,
        Hedy,
        Arc<dyn crate::application::ports::Store>,
        PathBuf,
    )| { (texts(store, None), texts(store, Some(project))) };
    let first = state(&pcs[0]);
    for pc in &pcs[1..] {
        assert_eq!(
            state(pc),
            first,
            "seed {seed} (sqlite: {sqlite}): the PCs disagree"
        );
    }
    for (i, (_, hedy, _, _)) in pcs.iter().enumerate() {
        let report = hedy.sync_now().unwrap();
        assert_eq!(
            report,
            SyncReport::default(),
            "seed {seed}: pc{i} still sends"
        );
    }
}

/// SQLite gives a deleted last row's id to the next one: deleting a note and
/// writing another before syncing is a deletion and a new note, not an edit.
#[test]
fn a_reused_row_id_is_a_new_note() {
    use std::sync::Arc;

    use crate::adapters::SqliteStore;
    use crate::application::testing::FakeSyncRemote;

    let tmp = tempfile::tempdir().unwrap();
    let server = SyncServer::default();
    let (f, _) = pc(&server, "/a/tienda");
    let store = Arc::new(SqliteStore::open(&tmp.path().join("hedy.db")).unwrap());
    let home = f.hedy_on(store.clone(), Arc::new(FakeSyncRemote::new(server.clone())));
    let (office, _) = pc(&server, "/b/tienda");

    let old = home
        .add_note(None, "Nota vieja", Category::Decision)
        .unwrap();
    let words = home.sync_setup(URL, "Casa").unwrap();
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();

    // Home deletes it and writes another, which gets the same row id.
    home.forget_note(old.id).unwrap();
    f.clock.advance(5);
    let new = home
        .add_note(None, "Nota nueva", Category::Decision)
        .unwrap();
    assert_eq!(new.id, old.id);
    // Meanwhile the office edits the old one.
    office.clock.advance(10);
    let there = office.store.notes(None, false).unwrap()[0].id;
    office
        .hedy
        .edit_note(there, "Nota vieja editada", Category::Decision)
        .unwrap();

    home.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    home.sync_now().unwrap();
    // The new note is its own item: it reaches the office whatever happens
    // to the old one, and nobody's edit turns into another note.
    let office_texts = texts(&office, None);
    assert!(
        office_texts.contains(&"Nota nueva".to_owned()),
        "{office_texts:?}"
    );
    let home_texts: Vec<String> = {
        let mut t: Vec<String> = store
            .notes(None, false)
            .unwrap()
            .into_iter()
            .map(|n| n.text)
            .collect();
        t.sort();
        t
    };
    assert_eq!(home_texts, office_texts);
}

// Regressions from the phase-2 review (docs/decisions.md §34).

#[test]
fn an_edit_survives_a_sync_that_fails() {
    let (_server, (home, _), (office, _)) = two_pcs();
    let note = home.hedy.add_note(None, "v1", Category::Decision).unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    let there = office.store.notes(None, false).unwrap()[0].id;

    // The push fails (no network): the edit is not lost.
    office
        .hedy
        .edit_note(there, "v2", Category::Decision)
        .unwrap();
    office.sync.fail_next_push();
    assert!(office.hedy.sync_now().is_err());
    office.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();
    assert_eq!(texts(&home, None), ["v2"]);

    // A batch that does not open stops the sync before anything is lost.
    let home_id = home.hedy.sync_status().unwrap().unwrap().device;
    let junk = layout::batch(&home_id, 99);
    home.sync.write(Path::new("/"), &junk, b"junk").unwrap();
    home.sync.push(Path::new("/"), "junk").unwrap();
    office
        .hedy
        .edit_note(there, "v3", Category::Decision)
        .unwrap();
    assert!(office.hedy.sync_now().is_err());
    home.sync.remove(Path::new("/"), &junk).unwrap();
    home.sync.push(Path::new("/"), "clean").unwrap();
    office.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();
    assert_eq!(texts(&home, None), ["v3"]);
    assert_eq!(note.text, "v1");
}

#[test]
fn a_new_epoch_does_not_resend_or_override_anything() {
    let server = SyncServer::default();
    let (home, home_tienda) = pc(&server, "/a/tienda");
    home.hedy
        .set_memory_home(&home_tienda, Some(&MemoryHome::Personal))
        .unwrap();
    home.hedy
        .add_template(NewTemplate {
            project: Some(home_tienda.clone()),
            name: "Revisar".into(),
            text: "v1".into(),
        })
        .unwrap();
    let words = home.hedy.sync_setup(URL, "Casa").unwrap();
    let (office, office_tienda) = pc(&server, "/b/tienda");
    office
        .hedy
        .set_memory_home(&office_tienda, Some(&MemoryHome::Personal))
        .unwrap();
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    // The office edits right after joining; home only then sees epoch 2.
    let record = office.store.template_records().unwrap()[0].clone();
    office
        .hedy
        .update_template(
            &crate::domain::TemplateId::Saved(record.id),
            "Revisar",
            "v2",
        )
        .unwrap();
    office.clock.advance(1);
    office.hedy.sync_now().unwrap();
    home.clock.advance(5);
    let report = home.hedy.sync_now().unwrap();
    assert_eq!(report.sent, 0, "home re-sent its items under the new epoch");
    let text = |f: &Fixture| f.store.template_records().unwrap()[0].text.clone();
    assert_eq!(text(&home), "v2");
    office.hedy.sync_now().unwrap();
    assert_eq!(text(&office), "v2");
}

#[test]
fn a_project_kept_local_for_a_while_catches_up() {
    let (_server, (home, home_tienda), (office, office_tienda)) = two_pcs();
    let note = home
        .hedy
        .add_note(Some(&home_tienda), "v1", Category::Decision)
        .unwrap();
    let other = home
        .hedy
        .add_note(Some(&home_tienda), "se borra", Category::Decision)
        .unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    assert_eq!(texts(&office, Some(&office_tienda)), ["se borra", "v1"]);

    // The office keeps the project local while home edits and deletes.
    office
        .hedy
        .set_memory_home(&office_tienda, Some(&MemoryHome::Local))
        .unwrap();
    home.hedy
        .edit_note(note.id, "v2", Category::Decision)
        .unwrap();
    home.hedy.forget_note(other.id).unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    assert_eq!(texts(&office, Some(&office_tienda)), ["se borra", "v1"]);

    // Personal again: it catches up, and sends nothing old back.
    office
        .hedy
        .set_memory_home(&office_tienda, Some(&MemoryHome::Personal))
        .unwrap();
    let report = office.hedy.sync_now().unwrap();
    assert_eq!(report.sent, 0);
    assert_eq!(texts(&office, Some(&office_tienda)), ["v2"]);
    home.hedy.sync_now().unwrap();
    assert_eq!(texts(&home, Some(&home_tienda)), ["v2"]);
}

#[test]
fn a_project_made_personal_later_brings_its_notes() {
    let (_server, (home, home_tienda), (office, office_tienda)) = two_pcs();
    office.hedy.set_memory_home(&office_tienda, None).unwrap();
    home.hedy
        .add_note(Some(&home_tienda), "de casa", Category::Decision)
        .unwrap();
    office
        .hedy
        .add_note(Some(&office_tienda), "de la oficina", Category::Decision)
        .unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    assert_eq!(texts(&office, Some(&office_tienda)), ["de la oficina"]);

    office
        .hedy
        .set_memory_home(&office_tienda, Some(&MemoryHome::Personal))
        .unwrap();
    office.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();
    for (f, project) in [(&home, &home_tienda), (&office, &office_tienda)] {
        assert_eq!(texts(f, Some(project)), ["de casa", "de la oficina"]);
    }
}

#[test]
fn two_themes_with_one_name_stay_two_themes() {
    let (_server, (home, _), (office, _)) = two_pcs();
    let theme = |f: &Fixture, bg: &str| {
        f.hedy
            .save_theme(NewTheme {
                id: None,
                name: "Noche".into(),
                base: ThemeBase::Dark,
                colors: BTreeMap::from([
                    ("bg".to_owned(), bg.to_owned()),
                    ("ink".to_owned(), "#f8f8f2".to_owned()),
                ]),
                origin: ThemeOrigin::Vscode,
            })
            .unwrap()
    };
    theme(&home, "#111111");
    theme(&office, "#222222");
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();
    for f in [&home, &office] {
        let mut colors: Vec<String> = f
            .hedy
            .list_themes()
            .unwrap()
            .into_iter()
            .map(|t| t.colors["bg"].clone())
            .collect();
        colors.sort();
        assert_eq!(colors, ["#111111", "#222222"]);
    }
    assert_eq!(home.hedy.sync_now().unwrap().sent, 0);
    assert_eq!(office.hedy.sync_now().unwrap().sent, 0);
}

#[test]
fn a_builtin_template_edited_on_both_pcs_is_one_template() {
    let (_server, (home, _), (office, _)) = two_pcs();
    let builtin: crate::domain::TemplateId = "builtin:tests".parse().unwrap();
    home.hedy
        .update_template(&builtin, "Tests", "versión de casa")
        .unwrap();
    office.clock.advance(10);
    office
        .hedy
        .update_template(&builtin, "Tests", "versión de la oficina")
        .unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();
    for f in [&home, &office] {
        let rows = f.store.template_records().unwrap();
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].text, "versión de la oficina");
    }
}

#[test]
fn a_changed_key_epoch_stops_the_sync() {
    let (server, (home, _), (office, _)) = two_pcs();
    home.hedy.sync_now().unwrap();
    // Someone with push access rewrites epoch 2 (here, only its spacing).
    let path = layout::epoch(2);
    let file = String::from_utf8(server.file(&path).unwrap()).unwrap();
    office
        .sync
        .write(Path::new("/"), &path, file.replace('\n', "\n ").as_bytes())
        .unwrap();
    office.sync.push(Path::new("/"), "tamper").unwrap();
    assert_eq!(home.hedy.sync_now(), Err(AppError::SyncTampered(2)));
}

#[test]
fn a_removed_pc_cannot_write_and_no_pc_writes_as_another() {
    use crate::application::ports::SyncCrypto;

    let (server, (home, _), (office, _)) = two_pcs();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    let crypto = crate::adapters::VaultCrypto;
    let office_keys: Keys =
        serde_json::from_str(&office.secrets.sync_keys().unwrap().unwrap()).unwrap();
    let home_id = home.hedy.sync_status().unwrap().unwrap().device;
    let office_id = office.hedy.sync_status().unwrap().unwrap().device;

    // Batches written with keys the PC does not own.
    let forged = |device: &DeviceId, seq: u64, signer: &str| {
        let stamp = Stamp {
            ms: 1,
            counter: 0,
            device: device.clone(),
        };
        let body = NoteBody {
            text: "inyectada".into(),
            category: Category::Rule,
            author: "Usuario".into(),
            status: NoteStatus::Approved,
            created_at: 1,
            topic: None,
            updated_at: None,
        };
        let batch = Batch {
            device: device.clone(),
            seq,
            changes: vec![Change {
                kind: ItemKind::Note,
                uid: "x".into(),
                scope: Scope::Global,
                version: Version::new(stamp, Some(json(&body))),
            }],
        };
        let path = layout::batch(device, seq);
        let body = json(&batch);
        let signature = crypto.sign(signer, &signed_bytes(&path, &body)).unwrap();
        let envelope = json(&Signed { body, signature });
        let key = office_keys.epochs.values().last().unwrap();
        let sealed = crypto.seal(key, &path, envelope.as_bytes()).unwrap();
        office.sync.write(Path::new("/"), &path, &sealed).unwrap();
        office.sync.push(Path::new("/"), "forged").unwrap();
    };
    // As the office (a member) but signed by a key that is not its own:
    // refused, nothing applied.
    let stranger = crypto.new_member().unwrap();
    forged(&office_id, 30, &stranger);
    assert!(home.hedy.sync_now().is_err());
    assert!(!texts(&home, None).contains(&"inyectada".to_owned()));
    office
        .sync
        .remove(Path::new("/"), &layout::batch(&office_id, 30))
        .unwrap();
    office.sync.push(Path::new("/"), "clean").unwrap();
    home.hedy.sync_now().unwrap();

    // Home removes the office (epoch 3), which then writes as a new PC
    // with the keys it still has.
    let keys: Keys = serde_json::from_str(&home.secrets.sync_keys().unwrap().unwrap()).unwrap();
    let epoch2 = String::from_utf8(server.file(&layout::epoch(2)).unwrap()).unwrap();
    let (_, members) = crypto.epoch_members(&epoch2).unwrap();
    let kept: Vec<(String, String)> = members
        .into_iter()
        .filter(|(id, _)| id != office_id.as_str())
        .collect();
    let (file, _) = crypto
        .new_epoch(3, &kept, home_id.as_str(), &keys.device)
        .unwrap();
    home.sync
        .write(Path::new("/"), &layout::epoch(3), file.as_bytes())
        .unwrap();
    home.sync.push(Path::new("/"), "remove office").unwrap();
    home.hedy.sync_now().unwrap();
    forged(&DeviceId::from_bytes([7; 8]), 1, &office_keys.device);
    forged(&office_id, 40, &office_keys.device);
    home.hedy.sync_now().unwrap();
    assert!(!texts(&home, None).contains(&"inyectada".to_owned()));
}
