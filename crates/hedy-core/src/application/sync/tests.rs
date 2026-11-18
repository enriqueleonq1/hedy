//! Two PCs (fixtures) syncing through one shared server.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::*;
use crate::application::NewTemplate;
use crate::application::ports::{
    Clock, MemoryRepository, SecretStore, SettingsRepository, SyncRemote, SyncRepository,
    TemplateRepository,
};
use crate::application::testing::{Fixture, SyncServer};
use crate::application::themes::NewTheme;
use crate::domain::sync::{MemoryHome, Owners};
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
    let members = crypto.epoch_members(&epoch2).unwrap().members;
    let home_id = home.hedy.sync_status().unwrap().unwrap().device;
    let kept: Vec<EpochMember> = members
        .into_iter()
        .filter(|m| m.id == RECOVERY_MEMBER || m.id == home_id.as_str())
        .collect();
    let (file, _) = crypto
        .new_epoch(3, false, &kept, home_id.as_str(), &keys.device)
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
    let next = home
        .store
        .get_setting(SEQ)
        .unwrap()
        .unwrap()
        .parse::<u64>()
        .unwrap()
        + 1;
    let junk = layout::batch(&home_id, next);
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
    office
        .hedy
        .set_memory_home(&office_tienda, Some(&MemoryHome::Local))
        .unwrap();
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

    // Back to following its owner, which the user says is theirs.
    office
        .hedy
        .answer_owner("github.com/tu-usuario", true)
        .unwrap();
    office.hedy.set_memory_home(&office_tienda, None).unwrap();
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
    let next = office
        .store
        .get_setting(SEQ)
        .unwrap()
        .unwrap()
        .parse::<u64>()
        .unwrap()
        + 1;
    forged(&office_id, next, &stranger);
    assert!(home.hedy.sync_now().is_err());
    assert!(!texts(&home, None).contains(&"inyectada".to_owned()));
    office
        .sync
        .remove(Path::new("/"), &layout::batch(&office_id, next))
        .unwrap();
    office.sync.push(Path::new("/"), "clean").unwrap();
    home.hedy.sync_now().unwrap();

    // Home removes the office (epoch 3), which then writes as a new PC
    // with the keys it still has.
    let keys: Keys = serde_json::from_str(&home.secrets.sync_keys().unwrap().unwrap()).unwrap();
    let epoch2 = String::from_utf8(server.file(&layout::epoch(2)).unwrap()).unwrap();
    let members = crypto.epoch_members(&epoch2).unwrap().members;
    let kept: Vec<EpochMember> = members
        .into_iter()
        .filter(|m| m.id != office_id.as_str())
        .collect();
    let (file, _) = crypto
        .new_epoch(3, false, &kept, home_id.as_str(), &keys.device)
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

#[test]
fn a_batch_hidden_for_a_while_is_still_applied_in_order() {
    let (server, (home, _), (office, _)) = two_pcs();
    home.hedy.add_note(None, "A", Category::Decision).unwrap();
    home.hedy.sync_now().unwrap();
    home.hedy.add_note(None, "B", Category::Decision).unwrap();
    home.hedy.sync_now().unwrap();
    // Someone with push access hides batch A for one sync.
    let home_id = home.hedy.sync_status().unwrap().unwrap().device;
    let seq_b = home
        .store
        .get_setting(SEQ)
        .unwrap()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let hidden = layout::batch(&home_id, seq_b - 1);
    let saved = server.file(&hidden).unwrap();
    office.sync.remove(Path::new("/"), &hidden).unwrap();
    office.sync.push(Path::new("/"), "hide").unwrap();
    office.hedy.sync_now().unwrap();
    assert!(texts(&office, None).is_empty(), "B was applied before A");
    office.sync.write(Path::new("/"), &hidden, &saved).unwrap();
    office.sync.push(Path::new("/"), "put back").unwrap();
    office.hedy.sync_now().unwrap();
    assert_eq!(texts(&office, None), ["A", "B"]);
}

#[test]
fn setup_returns_the_words_even_when_its_first_sync_fails() {
    let server = SyncServer::default();
    let (home, _) = pc(&server, "/a/tienda");
    home.hedy
        .add_note(None, "nota", Category::Decision)
        .unwrap();
    // The epoch is pushed; the first sync's push fails.
    home.sync.fail_push_after(1);
    let words = home.hedy.sync_setup(URL, "Casa").unwrap();
    assert_eq!(words.split_whitespace().count(), 12);
    home.hedy.sync_now().unwrap();
    let (office, _) = pc(&server, "/b/tienda");
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    assert_eq!(texts(&office, None), ["nota"]);
}

#[test]
fn junk_in_the_epochs_folder_is_ignored_and_bad_pins_fail_closed() {
    let (_server, (home, _), (office, _)) = two_pcs();
    office
        .sync
        .write(Path::new("/"), "vault/epochs/999999.json", b"junk")
        .unwrap();
    office.sync.push(Path::new("/"), "junk").unwrap();
    home.hedy.sync_now().unwrap();
    home.store.set_setting(PINS, Some("no es json")).unwrap();
    assert!(home.hedy.sync_now().is_err());
}

// Regressions from the second review of phase 2.

#[test]
fn a_body_this_version_cannot_read_waits_without_stopping_the_sync() {
    use crate::application::ports::SyncCrypto;

    let (_server, (home, _), (office, _)) = two_pcs();
    home.hedy.sync_now().unwrap();
    let there = office
        .hedy
        .add_note(None, "de la oficina", Category::Decision)
        .unwrap();
    office.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();

    // Home (a newer Hedy) sends a note with a category this one lacks.
    let crypto = crate::adapters::VaultCrypto;
    let keys: Keys = serde_json::from_str(&home.secrets.sync_keys().unwrap().unwrap()).unwrap();
    let home_id = home.hedy.sync_status().unwrap().unwrap().device;
    let seq = home
        .store
        .get_setting(SEQ)
        .unwrap()
        .unwrap()
        .parse::<u64>()
        .unwrap()
        + 1;
    home.store.set_setting(SEQ, Some(&seq.to_string())).unwrap();
    let future = r#"{"text":"futura","category":"futureCategory","author":"x","status":"approved","createdAt":1}"#;
    let batch = Batch {
        device: home_id.clone(),
        seq,
        changes: vec![Change {
            kind: ItemKind::Note,
            uid: "futura".into(),
            scope: Scope::Global,
            version: Version::new(
                Stamp {
                    ms: home.clock.now_ms(),
                    counter: 9,
                    device: home_id.clone(),
                },
                Some(future.into()),
            ),
        }],
    };
    let path = layout::batch(&home_id, seq);
    let body = json(&batch);
    let signature = crypto
        .sign(&keys.device, &signed_bytes(&path, &body))
        .unwrap();
    let sealed = crypto
        .seal(
            keys.epochs.values().last().unwrap(),
            &path,
            json(&Signed { body, signature }).as_bytes(),
        )
        .unwrap();
    home.sync.write(Path::new("/"), &path, &sealed).unwrap();
    home.sync.push(Path::new("/"), "future").unwrap();

    // The office keeps syncing and its edits still leave.
    office
        .hedy
        .edit_note(there.id, "editada en la oficina", Category::Decision)
        .unwrap();
    office.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();
    assert_eq!(texts(&home, None), ["editada en la oficina"]);
    assert!(!texts(&office, None).contains(&"futura".to_owned()));
}

#[test]
fn a_remote_renamed_on_one_pc_does_not_ping_pong() {
    let (_server, (home, home_tienda), (office, office_tienda)) = two_pcs();
    let note = home
        .hedy
        .add_note(Some(&home_tienda), "v1", Category::Decision)
        .unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();

    // Home's origin now has the repo's new name; the office's does not.
    home.source
        .set_remotes(&[("origin", "git@github.com:tu-usuario/tienda-nueva.git")]);
    let mut sent = Vec::new();
    for _ in 0..3 {
        sent.push(home.hedy.sync_now().unwrap().sent);
        sent.push(office.hedy.sync_now().unwrap().sent);
    }
    assert_eq!(sent, [0, 0, 0, 0, 0, 0]);

    // An edit made at home still reaches the office's project.
    home.hedy
        .edit_note(note.id, "v2", Category::Decision)
        .unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    assert_eq!(texts(&office, Some(&office_tienda)), ["v2"]);
}

#[test]
fn a_pending_batch_replayed_after_a_crash_keeps_its_seq() {
    let (server, (home, _), (office, _)) = two_pcs();
    home.hedy.sync_now().unwrap();
    // A crash left a pending batch whose seq never reached `sync.seq`.
    let seq = home
        .store
        .get_setting(SEQ)
        .unwrap()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let pending = Pending {
        seq: seq + 1,
        outgoing: Vec::new(),
    };
    home.store
        .set_setting(PENDING, Some(&json(&pending)))
        .unwrap();
    home.hedy.add_note(None, "B", Category::Decision).unwrap();
    home.hedy.sync_now().unwrap();
    let home_id = home.hedy.sync_status().unwrap().unwrap().device;
    assert!(server.file(&layout::batch(&home_id, seq + 1)).is_some());
    assert!(server.file(&layout::batch(&home_id, seq + 2)).is_some());
    office.hedy.sync_now().unwrap();
    assert_eq!(texts(&office, None), ["B"]);
}

// Phase 3: where each project's memory lives.

#[test]
fn a_new_owners_project_waits_for_the_answer() {
    let server = SyncServer::default();
    let (home, home_app) = pc(&server, "/a/app");
    let (office, office_app) = pc(&server, "/b/app");
    for f in [&home, &office] {
        f.source
            .set_remotes(&[("origin", "git@github.com:empresa-org/app.git")]);
    }
    let words = home.hedy.sync_setup(URL, "Casa").unwrap();
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    home.hedy
        .add_note(Some(&home_app), "Regla del cliente", Category::Rule)
        .unwrap();
    let status = home.hedy.memory_home_status(&home_app).unwrap();
    assert_eq!(status.ask.as_deref(), Some("github.com/empresa-org"));

    // Not answered, or shared: it never leaves this PC.
    assert_eq!(home.hedy.sync_now().unwrap().sent, 0);
    home.hedy
        .answer_owner("github.com/empresa-org", false)
        .unwrap();
    assert_eq!(home.hedy.sync_now().unwrap().sent, 0);

    // The user's own after all: it syncs where the owner is also the user's.
    home.hedy
        .answer_owner("github.com/empresa-org", true)
        .unwrap();
    assert_eq!(home.hedy.sync_now().unwrap().sent, 1);
    office.hedy.sync_now().unwrap();
    assert!(texts(&office, Some(&office_app)).is_empty());
    office
        .hedy
        .answer_owner("github.com/empresa-org", true)
        .unwrap();
    office.hedy.sync_now().unwrap();
    assert_eq!(texts(&office, Some(&office_app)), ["Regla del cliente"]);
}

#[test]
fn nobody_is_the_users_until_the_user_says_so() {
    let (_server, (home, home_tienda), _) = two_pcs();
    home.hedy.set_memory_home(&home_tienda, None).unwrap();
    let status = home.hedy.memory_home_status(&home_tienda).unwrap();
    assert_eq!(status.ask.as_deref(), Some("github.com/tu-usuario"));
    assert_eq!(home.hedy.repo_owners().unwrap(), Owners::default());
}

#[test]
fn a_sync_repo_in_an_organization_keeps_its_projects_shared() {
    let server = SyncServer::default();
    let (home, app) = pc(&server, "/a/app");
    home.source
        .set_remotes(&[("origin", "git@github.com:empresa-org/app.git")]);
    home.hedy
        .answer_owner("github.com/empresa-org", false)
        .unwrap();
    home.hedy
        .add_note(Some(&app), "Regla de la empresa", Category::Rule)
        .unwrap();
    home.hedy
        .sync_setup("git@github.com:empresa-org/hedy-memory.git", "Casa")
        .unwrap();
    assert_eq!(
        home.hedy.memory_home_status(&app).unwrap().home,
        Some(MemoryHome::Shared("github.com/empresa-org".into()))
    );
    assert_eq!(home.hedy.sync_now().unwrap().sent, 0);
}

#[test]
fn the_same_owner_on_a_company_server_is_not_the_user() {
    let (_server, (home, _), (office, _)) = two_pcs();
    home.hedy
        .answer_owner("github.com/tu-usuario", true)
        .unwrap();
    home.fs.add_git_root("/a/interna");
    let interna = home.dir("/a/interna");
    home.hedy.use_account(&interna, "personal", None).unwrap();
    home.source.set_remotes_of(
        &interna,
        &[("origin", "git@gitlab.empresa.com:tu-usuario/interna.git")],
    );
    home.hedy
        .add_note(Some(&interna), "Código de la empresa", Category::Rule)
        .unwrap();
    assert_eq!(home.hedy.sync_now().unwrap().sent, 0);
    office.hedy.sync_now().unwrap();
    assert!(texts(&office, None).is_empty());
}

#[test]
fn random_memory_homes_never_leak_a_shared_project() {
    for seed in 1..=8u64 {
        let server = SyncServer::default();
        let mut pcs = Vec::new();
        for (i, root) in ["/a", "/b", "/c"].iter().enumerate() {
            let (f, tienda) = pc(&server, &format!("{root}/tienda"));
            let cliente = f.dir(&format!("{root}/cliente"));
            f.fs.add_git_root(cliente.to_str().unwrap());
            f.hedy.use_account(&cliente, "personal", None).unwrap();
            f.source.set_remotes_of(
                &cliente,
                &[("origin", "git@github.com:cliente-sa/portal.git")],
            );
            f.hedy.answer_owner("github.com/cliente-sa", false).unwrap();
            // "Following its owner" makes "tienda" personal.
            f.hedy.answer_owner("github.com/tu-usuario", true).unwrap();
            assert_eq!(
                f.hedy.memory_home_status(&cliente).unwrap().home,
                Some(MemoryHome::Shared("github.com/cliente-sa".into()))
            );
            pcs.push((f, tienda, cliente, i));
        }
        let words = pcs[0].0.hedy.sync_setup(URL, "pc0").unwrap();
        for (f, _, _, i) in pcs.iter().skip(1) {
            f.hedy.sync_join(URL, &words, &format!("pc{i}")).unwrap();
        }

        let mut rng = Lcg(seed);
        let mut written = 0;
        for _ in 0..150 {
            let (f, tienda, cliente, i) = &pcs[rng.next(pcs.len())];
            f.clock.advance(rng.next(5) as i64);
            match rng.next(7) {
                0 | 1 => {
                    written += 1;
                    f.hedy
                        .add_note(
                            Some(tienda),
                            &format!("tienda {seed}-{written}"),
                            Category::Decision,
                        )
                        .unwrap();
                }
                2 => {
                    written += 1;
                    f.hedy
                        .add_note(
                            Some(cliente),
                            &format!("cliente pc{i} {seed}-{written}"),
                            Category::Rule,
                        )
                        .unwrap();
                }
                3 => {
                    let notes = f.store.notes(Some(tienda), false).unwrap();
                    if !notes.is_empty() {
                        written += 1;
                        let note = &notes[rng.next(notes.len())];
                        f.hedy
                            .edit_note(
                                note.id,
                                &format!("editada {seed}-{written}"),
                                Category::Decision,
                            )
                            .unwrap();
                    }
                }
                4 => {
                    let home = match rng.next(3) {
                        0 => Some(MemoryHome::Personal),
                        1 => Some(MemoryHome::Local),
                        _ => None,
                    };
                    f.hedy.set_memory_home(tienda, home.as_ref()).unwrap();
                }
                _ => {
                    f.hedy.sync_now().unwrap();
                }
            }
            // Privacy, at every step: client notes stay on the PC that wrote them.
            for (other, _, other_cliente, j) in &pcs {
                for text in texts(other, Some(other_cliente)) {
                    assert!(
                        text.starts_with(&format!("cliente pc{j} ")),
                        "seed {seed}: {text} leaked"
                    );
                }
                let all: Vec<String> = other
                    .store
                    .notes(None, false)
                    .unwrap()
                    .into_iter()
                    .map(|n| n.text)
                    .collect();
                assert!(
                    all.iter().all(|t| !t.starts_with("cliente")),
                    "seed {seed}: leaked as global"
                );
            }
        }

        for (f, tienda, _, _) in &pcs {
            f.hedy
                .set_memory_home(tienda, Some(&MemoryHome::Personal))
                .unwrap();
        }
        for _ in 0..2 {
            for (f, _, _, _) in &pcs {
                f.hedy.sync_now().unwrap();
            }
        }
        let first = texts(&pcs[0].0, Some(&pcs[0].1));
        for (f, tienda, _, i) in &pcs[1..] {
            assert_eq!(
                texts(f, Some(tienda)),
                first,
                "seed {seed}: pc{i} disagrees"
            );
        }
        for (f, _, _, i) in &pcs {
            assert_eq!(
                f.hedy.sync_now().unwrap().sent,
                0,
                "seed {seed}: pc{i} still sends"
            );
        }
        // The server never holds a client note's text.
        for path in server.paths() {
            let bytes = server.file(&path).unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("cliente pc"));
        }
    }
}

// Phase 4.

#[test]
fn leaving_keeps_the_notes_and_joining_again_starts_fresh() {
    let (_server, (home, _), (office, _)) = two_pcs();
    home.hedy
        .add_note(None, "nota", Category::Decision)
        .unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();

    assert_eq!(office.hedy.sync_leave(), Ok(true));
    assert!(!office.hedy.sync_is_set_up().unwrap());
    assert_eq!(office.secrets.sync_keys().unwrap(), None);
    assert!(office.store.sync_items().unwrap().is_empty());
    assert_eq!(texts(&office, None), ["nota"]);
    // Leaving again finds nothing to clear.
    assert_eq!(office.hedy.sync_leave(), Ok(false));
    assert_eq!(office.hedy.sync_now(), Err(AppError::SyncNotSetUp));
}

#[test]
fn a_pc_that_joins_with_the_same_notes_adopts_them() {
    let server = SyncServer::default();
    let (home, _) = pc(&server, "/a/tienda");
    home.hedy
        .add_note(None, "Usa tabs", Category::Convention)
        .unwrap();
    let words = home.hedy.sync_setup(URL, "Casa").unwrap();
    // The office already has the same note (it left, or was copied).
    let (office, _) = pc(&server, "/b/tienda");
    office
        .store
        .write_synced_note(
            None,
            None,
            &NoteBody {
                text: "Usa tabs".into(),
                ..serde_json::from_str(
                    home.store.sync_items().unwrap()[0]
                        .version
                        .body
                        .as_deref()
                        .unwrap(),
                )
                .unwrap()
            },
        )
        .unwrap();
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    home.hedy.sync_now().unwrap();
    for f in [&home, &office] {
        assert_eq!(texts(f, None), ["Usa tabs"]);
    }
    assert_eq!(office.hedy.sync_now().unwrap().sent, 0);

    // It leaves and joins again: still one note everywhere.
    office.hedy.sync_leave().unwrap();
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    home.hedy.sync_now().unwrap();
    for f in [&home, &office] {
        assert_eq!(texts(f, None), ["Usa tabs"]);
    }
}

#[test]
fn joining_again_after_an_edit_elsewhere_adopts_the_row() {
    let server = SyncServer::default();
    let (home, _) = pc(&server, "/a/tienda");
    let words = home.hedy.sync_setup(URL, "Casa").unwrap();
    let (office, _) = pc(&server, "/b/tienda");
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    let note = home.hedy.add_note(None, "A", Category::Convention).unwrap();
    home.hedy.sync_now().unwrap();
    home.hedy
        .edit_note(note.id, "B", Category::Convention)
        .unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();

    // Version "A" comes in first and makes a row; version "B" then finds
    // the office's own "B" and moves onto it.
    office.hedy.sync_leave().unwrap();
    let report = office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    assert_eq!(report.sent, 0);
    home.hedy.sync_now().unwrap();
    for f in [&home, &office] {
        assert_eq!(texts(f, None), ["B"]);
    }
}

#[test]
fn a_url_with_a_token_is_refused_and_a_cut_short_setup_can_be_cleared() {
    let server = SyncServer::default();
    let (home, _) = pc(&server, "/a/tienda");
    for url in [
        "https://alice:ghp_TOKEN@github.com/tu-usuario/hedy-memory.git",
        "https://ghp_TOKEN@github.com/tu-usuario/hedy-memory.git",
    ] {
        assert_eq!(
            home.hedy.sync_setup(url, "Casa"),
            Err(AppError::SyncUrlWithCredentials)
        );
        assert_eq!(
            home.hedy.sync_join(url, "x", "Casa"),
            Err(AppError::SyncUrlWithCredentials)
        );
    }
    assert!(home.store.get_setting(super::URL).unwrap().is_none());
    // Keys left by a setup cut short (no settings yet) are still cleared.
    home.secrets.set_sync_keys("{}").unwrap();
    assert_eq!(home.hedy.sync_leave(), Ok(false));
    assert_eq!(home.secrets.sync_keys().unwrap(), None);
}

#[test]
fn a_failed_sync_is_remembered_until_one_works_and_a_public_repo_is_told() {
    let (_server, (home, _), _) = two_pcs();
    home.sync.fail_next_push();
    home.hedy
        .add_note(None, "nota", Category::Decision)
        .unwrap();
    assert!(home.hedy.sync_now().is_err());
    let error = home.hedy.sync_status().unwrap().unwrap().last_error;
    assert!(error.is_some_and(|e| e.contains("sin conexión")));
    home.hedy.sync_now().unwrap();
    assert_eq!(home.hedy.sync_status().unwrap().unwrap().last_error, None);

    assert_eq!(home.hedy.sync_repo_is_public(URL), Ok(false));
    assert_eq!(
        home.hedy
            .sync_repo_is_public("https://github.com/u/public-memory.git"),
        Ok(true)
    );
    assert_eq!(
        home.hedy
            .sync_repo_is_public("https://tok@github.com/u/m.git"),
        Err(AppError::SyncUrlWithCredentials)
    );
    assert_eq!(home.hedy.default_device_name(), "PC de prueba");
}

#[test]
fn an_error_is_kept_on_one_short_line_without_escapes() {
    let error = AppError::Port(crate::application::ports::PortError::new(format!(
        "git push falló:\nremote: \u{1b}[31mACCESO DENEGADO\u{1b}[0m\r\n{}",
        "x".repeat(400)
    )));
    let shown = super::shown_error(&error);
    assert!(shown.starts_with("git push falló: remote: ACCESO DENEGADO x"));
    assert!(!shown.contains('\u{1b}') && !shown.contains('\n'));
    assert_eq!(shown.chars().count(), super::ERROR_MAX + 1);
}

// Phase 6.

/// Home sets up; office and laptop join.
fn three_pcs() -> (SyncServer, Fixture, Fixture, Fixture) {
    let server = SyncServer::default();
    let (home, _) = pc(&server, "/a/tienda");
    let (office, _) = pc(&server, "/b/tienda");
    let (laptop, _) = pc(&server, "/c/tienda");
    let words = home.hedy.sync_setup(URL, "Casa").unwrap();
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    laptop.hedy.sync_join(URL, &words, "Portátil").unwrap();
    home.hedy.sync_now().unwrap();
    (server, home, office, laptop)
}

fn names(f: &Fixture) -> Vec<String> {
    f.hedy
        .sync_status()
        .unwrap()
        .unwrap()
        .devices
        .into_iter()
        .map(|d| d.name)
        .collect()
}

#[test]
fn a_removed_pc_reads_nothing_new_and_what_it_wrote_last_still_arrives() {
    let (server, home, office, laptop) = three_pcs();
    office.hedy.add_note(None, "A", Category::Decision).unwrap();
    office.hedy.sync_now().unwrap();
    laptop.hedy.sync_now().unwrap();
    // B reaches the repo, but the laptop has not read it when office goes.
    office.hedy.add_note(None, "B", Category::Decision).unwrap();
    office.hedy.sync_now().unwrap();

    assert_eq!(
        home.hedy.sync_remove_device("Casa"),
        Err(AppError::SyncRemoveThisPc)
    );
    assert_eq!(
        home.hedy.sync_remove_device("Nadie"),
        Err(AppError::SyncDeviceNotFound("Nadie".into()))
    );
    home.hedy.sync_remove_device("Oficina").unwrap();
    assert_eq!(names(&home), ["Casa", "Portátil"]);
    let office_id = office.hedy.sync_status().unwrap().unwrap().device;
    // Only its name stays (a commit it has not pushed still rebases).
    let left: Vec<String> = server
        .paths()
        .into_iter()
        .filter(|p| p.starts_with(&layout::device_dir(&office_id)))
        .collect();
    assert_eq!(left, [layout::device_info(&office_id)]);

    laptop.hedy.sync_now().unwrap();
    assert_eq!(texts(&laptop, None), ["A", "B"]);
    assert_eq!(names(&laptop), ["Portátil", "Casa"]);

    // Office cannot read what is written from now on, nor write.
    home.hedy.add_note(None, "C", Category::Decision).unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.add_note(None, "D", Category::Decision).unwrap();
    assert_eq!(office.hedy.sync_now(), Err(AppError::SyncRemoved));
    assert_eq!(texts(&office, None), ["A", "B", "D"]);
    laptop.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();
    for f in [&home, &laptop] {
        assert_eq!(texts(f, None), ["A", "B", "C"]);
    }
    // Taking over sent each of its items once, and nothing more after.
    assert_eq!(home.hedy.sync_now().unwrap().sent, 0);
    assert_eq!(laptop.hedy.sync_now().unwrap().sent, 0);
}

#[test]
fn many_syncs_leave_a_snapshot_and_a_pc_behind_or_a_new_one_catches_up() {
    let server = SyncServer::default();
    let (home, _) = pc(&server, "/a/tienda");
    let words = home.hedy.sync_setup(URL, "Casa").unwrap();
    let (office, _) = pc(&server, "/b/tienda");
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    let mut first = None;
    for i in 0..12 {
        let note = home
            .hedy
            .add_note(None, &format!("nota {i:02}"), Category::Decision)
            .unwrap();
        first.get_or_insert(note.id);
        home.hedy.sync_now().unwrap();
    }
    // An edit and a deletion travel in the snapshot too.
    home.hedy
        .edit_note(first.unwrap(), "nota 00 editada", Category::Decision)
        .unwrap();
    let last = home.store.notes(None, false).unwrap();
    let last = last.iter().find(|n| n.text == "nota 11").unwrap().id;
    home.hedy.forget_note(last).unwrap();
    home.hedy.sync_now().unwrap();

    let home_id = home.hedy.sync_status().unwrap().unwrap().device;
    let own: Vec<String> = server
        .paths()
        .into_iter()
        .filter(|p| p.starts_with(&layout::device_dir(&home_id)))
        .collect();
    let batches = own
        .iter()
        .filter(|p| layout::batch_seq(p).is_some())
        .count();
    let snapshots = own
        .iter()
        .filter(|p| layout::snapshot_seq(p).is_some())
        .count();
    assert!(batches <= super::COMPACT_AFTER, "{own:?}");
    assert_eq!(snapshots, 1, "{own:?}");

    let mut expected: Vec<String> = (1..11).map(|i| format!("nota {i:02}")).collect();
    expected.insert(0, "nota 00 editada".into());
    // Behind since it joined, and new.
    office.hedy.sync_now().unwrap();
    let (laptop, _) = pc(&server, "/c/tienda");
    laptop.hedy.sync_join(URL, &words, "Portátil").unwrap();
    for f in [&office, &laptop] {
        assert_eq!(texts(f, None), expected);
        assert_eq!(f.hedy.sync_now().unwrap().sent, 0);
    }
}

// Phase 6 housekeeping: what a stolen PC (still a member, or with push
// access) and races must not do.

/// The office's keys and the newest epoch it knows.
fn office_keys(office: &Fixture) -> Keys {
    serde_json::from_str(&office.secrets.sync_keys().unwrap().unwrap()).unwrap()
}

#[test]
fn a_member_cannot_add_a_hidden_member_and_a_bad_epoch_stops_no_sync() {
    use crate::application::ports::SyncCrypto;
    let (server, home, office, laptop) = three_pcs();
    office.hedy.sync_now().unwrap();
    let crypto = crate::adapters::VaultCrypto;
    let keys = office_keys(&office);
    let office_id = office.hedy.sync_status().unwrap().unwrap().device;
    let newest = *keys.epochs.keys().last().unwrap();
    let file = String::from_utf8(server.file(&layout::epoch(newest)).unwrap()).unwrap();
    let mut members = crypto.epoch_members(&file).unwrap().members;
    // The (stolen) office signs an epoch that adds a member it controls.
    let phantom = crypto.new_member().unwrap();
    members.push(EpochMember::new(
        "abcdef0123456789",
        crypto.member_public(&phantom).unwrap(),
    ));
    let (next, _) = crypto
        .new_epoch(
            newest + 1,
            false,
            &members,
            office_id.as_str(),
            &keys.device,
        )
        .unwrap();
    office
        .sync
        .write(Path::new("/"), &layout::epoch(newest + 1), next.as_bytes())
        .unwrap();
    office.sync.push(Path::new("/"), "phantom").unwrap();
    // Not trusted, and it stops nobody.
    home.hedy.sync_now().unwrap();
    laptop.hedy.sync_now().unwrap();
    assert_eq!(names(&home), ["Casa", "Oficina", "Portátil"]);

    // Removing the office writes over it: the phantom opens nothing.
    home.hedy.sync_remove_device("Oficina").unwrap();
    laptop.hedy.sync_now().unwrap();
    let last = String::from_utf8(server.file(&layout::epoch(newest + 1)).unwrap()).unwrap();
    assert!(
        crypto
            .open_epoch(&last, "abcdef0123456789", &phantom)
            .is_err()
    );
    assert_eq!(names(&laptop), ["Portátil", "Casa"]);
    home.hedy
        .add_note(None, "tras quitar", Category::Decision)
        .unwrap();
    home.hedy.sync_now().unwrap();
    laptop.hedy.sync_now().unwrap();
    assert_eq!(texts(&laptop, None), ["tras quitar"]);
}

/// A template edited on the office, then later on the laptop (whose push
/// fails); then home removes the office or just syncs.
fn later_edit_after(remove: bool) -> String {
    let (_server, home, office, laptop) = three_pcs();
    let builtin: crate::domain::TemplateId = "builtin:tests".parse().unwrap();
    office.hedy.sync_now().unwrap();
    laptop.hedy.sync_now().unwrap();
    office.clock.advance(5);
    office
        .hedy
        .update_template(&builtin, "Tests", "oficina (antes)")
        .unwrap();
    office.hedy.sync_now().unwrap();
    laptop.clock.advance(10);
    laptop
        .hedy
        .update_template(&builtin, "Tests", "portátil (después)")
        .unwrap();
    laptop.sync.fail_next_push();
    assert!(laptop.hedy.sync_now().is_err());
    home.clock.advance(20);
    if remove {
        home.hedy.sync_remove_device("Oficina").unwrap();
    } else {
        home.hedy.sync_now().unwrap();
    }
    laptop.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();
    let rows = home.store.template_records().unwrap();
    rows.iter()
        .find(|r| r.builtin.as_deref() == Some("tests"))
        .map(|r| r.text.clone())
        .unwrap_or_default()
}

#[test]
fn taking_over_keeps_who_wins_a_conflict() {
    assert_eq!(later_edit_after(false), "portátil (después)");
    assert_eq!(later_edit_after(true), "portátil (después)");
}

#[test]
fn junk_in_a_members_folder_stops_no_sync_and_its_owner_clears_it() {
    let (server, home, office, laptop) = three_pcs();
    let home_id = home.hedy.sync_status().unwrap().unwrap().device;
    home.hedy.sync_remove_device("Oficina").unwrap();
    laptop.hedy.sync_now().unwrap();
    // The removed office still has push access.
    let junk = layout::snapshot(&home_id, 9_999_999_999);
    office.sync.write(Path::new("/"), &junk, b"junk").unwrap();
    office.sync.push(Path::new("/"), "junk").unwrap();
    laptop.hedy.sync_now().unwrap();
    home.hedy
        .add_note(None, "sigue", Category::Decision)
        .unwrap();
    home.hedy.sync_now().unwrap();
    assert!(!server.paths().contains(&junk));
    laptop.hedy.sync_now().unwrap();
    assert_eq!(texts(&laptop, None), ["sigue"]);
}

#[test]
fn an_id_that_is_not_a_pc_removes_nothing() {
    let (server, home, _office, _laptop) = three_pcs();
    let before = server.paths();
    for who in ["", "   ", "devices"] {
        assert!(matches!(
            home.hedy.sync_remove_device(who),
            Err(AppError::SyncDeviceNotFound(_))
        ));
    }
    let after = server.paths();
    assert!(
        before
            .iter()
            .filter(|p| p.starts_with("devices/"))
            .all(|p| after.contains(p))
    );
}

#[test]
fn taking_over_leaves_a_waiting_item_alone() {
    let server = SyncServer::default();
    let (home, home_tienda) = pc(&server, "/a/tienda");
    let (office, office_tienda) = pc(&server, "/b/tienda");
    let (laptop, laptop_tienda) = pc(&server, "/c/tienda");
    for (f, p) in [
        (&home, &home_tienda),
        (&office, &office_tienda),
        (&laptop, &laptop_tienda),
    ] {
        f.hedy
            .set_memory_home(p, Some(&MemoryHome::Personal))
            .unwrap();
    }
    let words = home.hedy.sync_setup(URL, "Casa").unwrap();
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    laptop.hedy.sync_join(URL, &words, "Portátil").unwrap();
    let note = office
        .hedy
        .add_note(Some(&office_tienda), "v1", Category::Decision)
        .unwrap();
    let gone = office
        .hedy
        .add_note(Some(&office_tienda), "se borra", Category::Decision)
        .unwrap();
    office.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();
    laptop.hedy.sync_now().unwrap();
    // Home keeps the project local a while: the office's edit waits there.
    home.hedy
        .set_memory_home(&home_tienda, Some(&MemoryHome::Local))
        .unwrap();
    office
        .hedy
        .edit_note(note.id, "v2", Category::Decision)
        .unwrap();
    office.hedy.forget_note(gone.id).unwrap();
    office.hedy.sync_now().unwrap();
    laptop.hedy.sync_now().unwrap();
    home.hedy.sync_remove_device("Oficina").unwrap();
    laptop.hedy.sync_now().unwrap();
    home.hedy
        .set_memory_home(&home_tienda, Some(&MemoryHome::Personal))
        .unwrap();
    home.hedy.sync_now().unwrap();
    laptop.hedy.sync_now().unwrap();
    for (f, p) in [(&home, &home_tienda), (&laptop, &laptop_tienda)] {
        assert_eq!(texts(f, Some(p)), ["v2"]);
    }
}

#[test]
fn taking_over_keeps_a_conflict_waiting_elsewhere() {
    let (_server, home, office, laptop) = three_pcs();
    let note = office
        .hedy
        .add_note(None, "base", Category::Decision)
        .unwrap();
    office.hedy.sync_now().unwrap();
    laptop.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();
    let id = laptop.store.notes(None, false).unwrap()[0].id;
    laptop
        .hedy
        .edit_note(id, "del portátil", Category::Decision)
        .unwrap();
    laptop.hedy.sync_now().unwrap();
    office.clock.advance(30);
    office
        .hedy
        .edit_note(note.id, "de la oficina", Category::Decision)
        .unwrap();
    office.hedy.sync_now().unwrap();
    laptop.hedy.sync_now().unwrap();
    assert_eq!(laptop.hedy.sync_conflicts().unwrap().len(), 1);
    home.clock.advance(60);
    home.hedy.sync_remove_device("Oficina").unwrap();
    laptop.hedy.sync_now().unwrap();
    assert_eq!(laptop.hedy.sync_conflicts().unwrap().len(), 1);
}

/// `GitSyncRemote` that runs something just before its next push.
struct BeforePush {
    inner: crate::adapters::GitSyncRemote,
    hook: std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl SyncRemote for BeforePush {
    fn lock(&self, dir: &Path) -> crate::application::ports::PortResult<SyncLock> {
        self.inner.lock(dir)
    }
    fn is_public(&self, url: &str) -> bool {
        self.inner.is_public(url)
    }
    fn device_name(&self) -> String {
        self.inner.device_name()
    }
    fn open(&self, url: &str, dir: &Path) -> crate::application::ports::PortResult<()> {
        self.inner.open(url, dir)
    }
    fn pull(&self, dir: &Path) -> crate::application::ports::PortResult<()> {
        self.inner.pull(dir)
    }
    fn files(
        &self,
        dir: &Path,
        prefix: &str,
    ) -> crate::application::ports::PortResult<Vec<String>> {
        self.inner.files(dir, prefix)
    }
    fn read(
        &self,
        dir: &Path,
        path: &str,
    ) -> crate::application::ports::PortResult<Option<Vec<u8>>> {
        self.inner.read(dir, path)
    }
    fn write(
        &self,
        dir: &Path,
        path: &str,
        bytes: &[u8],
    ) -> crate::application::ports::PortResult<()> {
        self.inner.write(dir, path, bytes)
    }
    fn remove(&self, dir: &Path, path: &str) -> crate::application::ports::PortResult<()> {
        self.inner.remove(dir, path)
    }
    fn push(&self, dir: &Path, message: &str) -> crate::application::ports::PortResult<()> {
        let hook = self.hook.lock().unwrap().take();
        if let Some(hook) = hook {
            hook();
        }
        self.inner.push(dir, message)
    }
    fn discard_local(&self, dir: &Path) -> crate::application::ports::PortResult<()> {
        self.inner.discard_local(dir)
    }
}

#[test]
fn a_removal_never_writes_over_an_epoch_that_came_in_meanwhile() {
    use crate::adapters::{GitSyncRemote, SqliteStore};
    use std::sync::Arc;
    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("hedy-memory.git");
    std::fs::create_dir_all(&bare).unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "--quiet", "--bare", "--initial-branch=main"])
            .current_dir(&bare)
            .status()
            .unwrap()
            .success()
    );
    let url = bare.to_string_lossy().into_owned();
    let racy = Arc::new(BeforePush {
        inner: GitSyncRemote,
        hook: Default::default(),
    });
    let make = |name: &str, remote: Arc<dyn SyncRemote>| {
        let f = Fixture::new();
        let data = tmp.path().join(name);
        std::fs::create_dir_all(&data).unwrap();
        let store = Arc::new(SqliteStore::open(&data.join("hedy.db")).unwrap());
        let hedy = f.hedy_on(store, remote);
        (f, hedy)
    };
    let (_home_f, home) = make("home", racy.clone());
    let (_office_f, office) = make("office", Arc::new(GitSyncRemote));
    let (_laptop_f, laptop) = make("laptop", Arc::new(GitSyncRemote));
    let words = home.sync_setup(&url, "Casa").unwrap();
    office.sync_join(&url, &words, "Oficina").unwrap();
    home.sync_now().unwrap();
    home.add_note(None, "algo que enviar", Category::Decision)
        .unwrap();
    let laptop = Arc::new(laptop);
    let (joining, url2, words2) = (laptop.clone(), url.clone(), words.clone());
    // The laptop joins (a new epoch) while home's first sync pushes.
    *racy.hook.lock().unwrap() = Some(Box::new(move || {
        joining.sync_join(&url2, &words2, "Portátil").unwrap();
    }));
    assert_eq!(home.sync_remove_device("Oficina"), Err(AppError::SyncBusy));
    laptop.sync_now().unwrap();
    // Again: it checks the laptop's epoch, then removes the office.
    home.sync_remove_device("Oficina").unwrap();
    laptop.sync_now().unwrap();
    let names: Vec<String> = laptop
        .sync_status()
        .unwrap()
        .unwrap()
        .devices
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert_eq!(names, ["Portátil", "Casa"]);
}

// Phase 6 housekeeping (correctness review).

#[test]
fn a_pc_far_behind_catches_up_through_a_snapshot_without_a_false_conflict() {
    let server = SyncServer::default();
    let (home, _) = pc(&server, "/a/tienda");
    let words = home.hedy.sync_setup(URL, "Casa").unwrap();
    let (office, _) = pc(&server, "/b/tienda");
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    let note = home.hedy.add_note(None, "v0", Category::Decision).unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    // More edits than a version's history holds, compacted away.
    for i in 1..=12 {
        home.clock.advance(5);
        home.hedy
            .edit_note(note.id, &format!("v{i}"), Category::Decision)
            .unwrap();
        home.hedy.sync_now().unwrap();
    }
    let report = office.hedy.sync_now().unwrap();
    assert_eq!(texts(&office, None), ["v12"]);
    assert_eq!(report.conflicts, 0);
    assert!(office.hedy.sync_conflicts().unwrap().is_empty());
}

#[test]
fn joining_again_after_the_others_compacted_adopts_the_row() {
    let server = SyncServer::default();
    let (home, _) = pc(&server, "/a/tienda");
    let words = home.hedy.sync_setup(URL, "Casa").unwrap();
    let (office, _) = pc(&server, "/b/tienda");
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    let note = home.hedy.add_note(None, "A", Category::Convention).unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    office.hedy.sync_leave().unwrap();
    home.hedy
        .edit_note(note.id, "B", Category::Convention)
        .unwrap();
    home.hedy.sync_now().unwrap();
    for i in 0..5 {
        home.hedy
            .add_template(NewTemplate {
                project: None,
                name: format!("t{i}"),
                text: format!("t{i}"),
            })
            .unwrap();
        home.hedy.sync_now().unwrap();
    }
    assert!(
        server
            .paths()
            .iter()
            .any(|p| layout::snapshot_seq(p).is_some())
    );
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    home.hedy.sync_now().unwrap();
    office.hedy.sync_now().unwrap();
    for f in [&home, &office] {
        assert_eq!(texts(f, None), ["B"]);
    }
}

#[test]
fn an_edit_made_on_the_removed_pcs_version_wins_over_the_take_over() {
    let (_server, home, office, laptop) = three_pcs();
    office
        .hedy
        .add_note(None, "uno", Category::Decision)
        .unwrap();
    let template = office
        .hedy
        .add_template(NewTemplate {
            project: None,
            name: "T".into(),
            text: "uno".into(),
        })
        .unwrap();
    let _ = template;
    office.hedy.sync_now().unwrap();
    laptop.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();
    // The laptop edits both, offline (its push fails), before the removal.
    let id = laptop.store.notes(None, false).unwrap()[0].id;
    laptop
        .hedy
        .edit_note(id, "dos", Category::Decision)
        .unwrap();
    let row = laptop
        .store
        .template_records()
        .unwrap()
        .into_iter()
        .find(|r| r.name == "T")
        .unwrap()
        .id;
    laptop
        .hedy
        .update_template(&crate::domain::TemplateId::Saved(row), "T", "dos")
        .unwrap();
    laptop.sync.fail_next_push();
    assert!(laptop.hedy.sync_now().is_err());
    home.clock.advance(30);
    home.hedy.sync_remove_device("Oficina").unwrap();
    laptop.hedy.sync_now().unwrap();
    home.hedy.sync_now().unwrap();
    laptop.hedy.sync_now().unwrap();
    for f in [&home, &laptop] {
        assert_eq!(texts(f, None), ["dos"]);
        let t = f.store.template_records().unwrap();
        assert_eq!(t.iter().find(|r| r.name == "T").unwrap().text, "dos");
        assert!(f.hedy.sync_conflicts().unwrap().is_empty());
    }
}

#[test]
fn an_epoch_without_the_recovery_key_is_ignored_and_written_over() {
    use crate::application::ports::SyncCrypto;
    let (server, home, office, laptop) = three_pcs();
    office.hedy.sync_now().unwrap();
    let crypto = crate::adapters::VaultCrypto;
    let keys = office_keys(&office);
    let office_id = office.hedy.sync_status().unwrap().unwrap().device;
    let newest = *keys.epochs.keys().last().unwrap();
    let file = String::from_utf8(server.file(&layout::epoch(newest)).unwrap()).unwrap();
    let members = crypto.epoch_members(&file).unwrap().members;
    let kept: Vec<_> = members
        .into_iter()
        .filter(|m| m.id != RECOVERY_MEMBER)
        .collect();
    let (next, _) = crypto
        .new_epoch(newest + 1, false, &kept, office_id.as_str(), &keys.device)
        .unwrap();
    office
        .sync
        .write(Path::new("/"), &layout::epoch(newest + 1), next.as_bytes())
        .unwrap();
    office
        .sync
        .push(Path::new("/"), "sin recuperación")
        .unwrap();
    home.hedy.sync_now().unwrap();
    laptop.hedy.sync_now().unwrap();
    home.hedy.sync_remove_device("Oficina").unwrap();
    laptop.hedy.sync_now().unwrap();
    assert_eq!(names(&laptop), ["Portátil", "Casa"]);
}

#[test]
fn junk_named_like_a_snapshot_never_replaces_the_real_one() {
    let server = SyncServer::default();
    let (home, _) = pc(&server, "/a/tienda");
    let words = home.hedy.sync_setup(URL, "Casa").unwrap();
    let (office, _) = pc(&server, "/b/tienda");
    office.hedy.sync_join(URL, &words, "Oficina").unwrap();
    for i in 0..5 {
        home.hedy
            .add_note(None, &format!("nota {i}"), Category::Decision)
            .unwrap();
        home.hedy.sync_now().unwrap();
    }
    let home_id = home.hedy.sync_status().unwrap().unwrap().device;
    let real = server
        .paths()
        .into_iter()
        .filter_map(|p| layout::snapshot_seq(&p))
        .map(|(_, s)| s)
        .max()
        .unwrap();
    // Named above the real one (but within the PC's seq), and the lowest.
    for junk in [real + 1, 0] {
        let path = layout::snapshot(&home_id, junk);
        office.sync.write(Path::new("/"), &path, b"junk").unwrap();
    }
    office.sync.push(Path::new("/"), "junk").unwrap();
    // A new PC reading now gets past the junk to the real one.
    let (laptop, _) = pc(&server, "/c/tienda");
    laptop.hedy.sync_join(URL, &words, "Portátil").unwrap();
    home.hedy.sync_now().unwrap();
    assert!(server.paths().contains(&layout::snapshot(&home_id, real)));
    let (desk, _) = pc(&server, "/d/tienda");
    desk.hedy.sync_join(URL, &words, "Escritorio").unwrap();
    let expected: Vec<String> = (0..5).map(|i| format!("nota {i}")).collect();
    for f in [&laptop, &desk] {
        assert_eq!(texts(f, None), expected);
    }
}

#[test]
fn setup_refuses_a_repo_with_files() {
    let server = SyncServer::default();
    let (home, _) = pc(&server, "/a/tienda");
    // Someone's code (a mistyped address), or a README made with the repo.
    home.sync
        .write(Path::new("/"), "README.md", b"# App")
        .unwrap();
    home.sync.push(Path::new("/"), "init").unwrap();
    assert_eq!(
        home.hedy.sync_setup(URL, "Casa"),
        Err(AppError::SyncRepoNotEmpty)
    );
    assert_eq!(server.file("README.md").unwrap(), b"# App");
    assert!(!home.hedy.sync_is_set_up().unwrap());
}

#[test]
fn the_recovery_words_never_join_a_teams_repo() {
    let server = SyncServer::default();
    let (office, _) = pc(&server, "/a/tienda");
    let crypto = crate::adapters::VaultCrypto;
    use crate::application::ports::SyncCrypto;
    // A team's repo (made by hand here: teams come in T2).
    let (words, recovery) = crypto.new_recovery().unwrap();
    let admin = crypto.new_member().unwrap();
    let members = [
        EpochMember::new(RECOVERY_MEMBER, crypto.member_public(&recovery).unwrap()),
        EpochMember {
            seat: Some(crate::domain::team::Seat {
                person: crate::domain::team::PersonId("aaaaaaaaaaaaaaaa".into()),
                role: crate::domain::team::Role::Admin,
            }),
            ..EpochMember::new("00000000000000a1", crypto.member_public(&admin).unwrap())
        },
    ];
    let (file, _) = crypto
        .new_epoch(1, true, &members, RECOVERY_MEMBER, &recovery)
        .unwrap();
    office
        .sync
        .write(Path::new("/"), &layout::epoch(1), file.as_bytes())
        .unwrap();
    office.sync.push(Path::new("/"), "team").unwrap();

    assert_eq!(
        office.hedy.sync_join(URL, &words, "Oficina"),
        Err(AppError::SyncTeamRepo)
    );
    assert_eq!(server.paths(), [layout::epoch(1)]);
    assert!(!office.hedy.sync_is_set_up().unwrap());
}
