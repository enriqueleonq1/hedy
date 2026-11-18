//! Personal sync (docs/decisions.md §34): the records each PC writes to the
//! sync repo and the rules that merge them. Pure: no I/O, no crypto (the
//! `vault` adapter seals what this module describes).
//!
//! Each PC only appends its own files, so git never has to merge anything;
//! what two PCs both changed is merged here, item by item:
//! - every change carries a [`Stamp`] (a hybrid logical clock: wall time,
//!   a counter and the PC) and the stamps of the versions it replaced;
//! - a change that builds on what a PC has applies; one it already has is
//!   ignored; two made apart are a [`Merge::Conflict`]: the newer wins and
//!   the other is kept for the user to choose.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// Stamps of the versions a change replaced that it carries, newest first:
/// enough to recognise a PC's own edits after a few more, small enough to
/// keep snapshots light.
pub const HISTORY_MAX: usize = 8;

/// A PC in the sync repo: random, never derived from the machine.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(pub String);

impl DeviceId {
    /// 16 lowercase hex characters, from 8 random bytes.
    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        Self(bytes.iter().map(|b| format!("{b:02x}")).collect())
    }

    /// A file-name-safe id: what [`Self::from_bytes`] makes.
    pub fn is_valid(&self) -> bool {
        self.0.len() == 16
            && self
                .0
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// When a change was made: wall time in milliseconds, a counter for changes
/// in the same millisecond (or after a clock went back), and the PC, so two
/// stamps are never equal unless they are the same change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Stamp {
    pub ms: i64,
    pub counter: u32,
    pub device: DeviceId,
}

impl Ord for Stamp {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.ms, self.counter, &self.device).cmp(&(other.ms, other.counter, &other.device))
    }
}

impl PartialOrd for Stamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Stamp {
    /// The stamp for a new change on `device` at `now_ms`, after `last` (the
    /// newest stamp this PC has made or seen): always greater than `last`,
    /// even when the wall clock went back.
    pub fn next(last: Option<&Stamp>, now_ms: i64, device: &DeviceId) -> Stamp {
        match last {
            Some(last) if last.ms >= now_ms => Stamp {
                ms: last.ms,
                counter: last.counter + 1,
                device: device.clone(),
            },
            _ => Stamp {
                ms: now_ms,
                counter: 0,
                device: device.clone(),
            },
        }
    }
}

/// What is synced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Note,
    Skill,
    Template,
    Theme,
}

impl ItemKind {
    /// Whether two versions made apart are shown to the user to choose
    /// (notes and skills); otherwise the newer simply wins.
    pub fn asks_on_conflict(self) -> bool {
        matches!(self, Self::Note | Self::Skill)
    }
}

/// Where an item belongs: everywhere, or one project, named by its
/// [`RepoIdentity`] sealed with the vault key (never its name or path).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Global,
    Project(String),
}

/// One version of an item. `body` is the item as the application encodes it
/// (opaque here); `None` is a deletion, kept so it reaches every PC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub stamp: Stamp,
    /// The versions this one replaced, newest first, at most
    /// [`HISTORY_MAX`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Stamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

impl Version {
    /// A first version (nothing before it).
    pub fn new(stamp: Stamp, body: Option<String>) -> Self {
        Self {
            stamp,
            history: Vec::new(),
            body,
        }
    }

    /// The version that replaces `self` with `body` at `stamp`.
    pub fn replaced(&self, stamp: Stamp, body: Option<String>) -> Self {
        let mut history = vec![self.stamp.clone()];
        history.extend(self.history.iter().take(HISTORY_MAX - 1).cloned());
        Self {
            stamp,
            history,
            body,
        }
    }

    pub fn is_deletion(&self) -> bool {
        self.body.is_none()
    }

    /// Whether `stamp` is this version or one it replaced.
    fn knows(&self, stamp: &Stamp) -> bool {
        &self.stamp == stamp || self.history.contains(stamp)
    }
}

/// One change to one item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub kind: ItemKind,
    /// The item's global id (random, the same on every PC).
    pub uid: String,
    pub scope: Scope,
    pub version: Version,
}

/// What to do with a version that came from another PC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Merge {
    /// Nothing here yet, or it builds on what is here: take it.
    Apply,
    /// It is here already, or what is here builds on it.
    Ignore,
    /// Made apart from what is here. The newer stamp wins; the loser is kept
    /// for the user ([`ItemKind::asks_on_conflict`]) or dropped.
    Conflict { incoming_wins: bool },
}

/// How `incoming` meets what this PC has of the same item.
pub fn merge(local: Option<&Version>, incoming: &Version) -> Merge {
    let Some(local) = local else {
        return Merge::Apply;
    };
    if local.knows(&incoming.stamp) {
        return Merge::Ignore;
    }
    if incoming.knows(&local.stamp) {
        return Merge::Apply;
    }
    // Two deletions agree, whoever made them.
    if local.is_deletion() && incoming.is_deletion() {
        return if incoming.stamp > local.stamp {
            Merge::Apply
        } else {
            Merge::Ignore
        };
    }
    Merge::Conflict {
        incoming_wins: incoming.stamp > local.stamp,
    }
}

/// A batch of changes one PC wrote: `seq` counts that PC's batches from 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Batch {
    pub device: DeviceId,
    pub seq: u64,
    pub changes: Vec<Change>,
}

/// Everything one PC knows, as of its batch `through_seq`: written now and
/// then so its older batches can be removed (compaction).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub device: DeviceId,
    pub through_seq: u64,
    pub changes: Vec<Change>,
}

/// Paths inside the sync repo. Everything but the README and the key
/// epochs is sealed, and no path names a project or an item.
pub mod layout {
    use super::DeviceId;

    pub const README: &str = "README.md";
    pub const EPOCHS_DIR: &str = "vault/epochs";
    pub const DEVICES_DIR: &str = "devices";

    /// The key file of `epoch` (1, 2, …), signed, not sealed.
    pub fn epoch(epoch: u32) -> String {
        format!("{EPOCHS_DIR}/{epoch:06}.json")
    }

    pub fn batch(device: &DeviceId, seq: u64) -> String {
        format!("{DEVICES_DIR}/{}/changes/{seq:010}.hedy", device.as_str())
    }

    pub fn snapshot(device: &DeviceId) -> String {
        format!("{DEVICES_DIR}/{}/snapshot.hedy", device.as_str())
    }

    /// The PC's name and when it last synced, sealed.
    pub fn device_info(device: &DeviceId) -> String {
        format!("{DEVICES_DIR}/{}/device.hedy", device.as_str())
    }

    /// The `seq` of a batch path; `None` for any other path.
    pub fn batch_seq(path: &str) -> Option<(DeviceId, u64)> {
        let rest = path.strip_prefix(DEVICES_DIR)?.strip_prefix('/')?;
        let (device, file) = rest.split_once("/changes/")?;
        let seq = file.strip_suffix(".hedy")?;
        let device = DeviceId(device.to_owned());
        if !device.is_valid() || seq.len() != 10 || !seq.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some((device, seq.parse().ok()?))
    }
}

/// Who owns a repo, read from a remote's URL: what tells a personal project
/// from a shared one, and the same project on two PCs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoIdentity {
    /// `github.com`, `gitlab.example.com`, or an SSH host alias as written.
    pub host: String,
    /// The owner: a user, an organization, or a group path
    /// (`group/subgroup` on GitLab). Lowercase.
    pub owner: String,
    /// Without `.git`. Lowercase.
    pub name: String,
}

impl RepoIdentity {
    /// Reads `https://host/owner/name(.git)`, `ssh://user@host:port/owner/name`,
    /// `user@host:owner/name` and `git://…`. `None` for a local path or a URL
    /// without an owner.
    pub fn from_url(url: &str) -> Option<Self> {
        let url = url.trim();
        let (host, path) = if let Some((scheme, rest)) = url.split_once("://") {
            if !matches!(scheme, "https" | "http" | "ssh" | "git" | "git+ssh") {
                return None;
            }
            let (authority, path) = rest.split_once('/')?;
            let host = authority.rsplit('@').next()?;
            let host = host.split(':').next()?;
            (host, path)
        } else {
            // scp-like: user@host:path (a Windows drive like C:\ is not one).
            let (authority, path) = url.split_once(':')?;
            if authority.contains('/') || authority.len() < 2 || path.starts_with('\\') {
                return None;
            }
            (authority.rsplit('@').next()?, path)
        };
        let path = path.trim_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        let (owner, name) = path.rsplit_once('/')?;
        let owner = owner.trim_start_matches('~');
        if host.is_empty() || owner.is_empty() || name.is_empty() {
            return None;
        }
        Some(Self {
            host: host.to_lowercase(),
            owner: owner.to_lowercase(),
            name: name.to_lowercase(),
        })
    }

    /// What matches the same project on two PCs: `owner/name`, without the
    /// host, so an SSH host alias (`github-work`) matches the real host.
    pub fn project_key(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(n: u8) -> DeviceId {
        DeviceId::from_bytes([n; 8])
    }

    fn stamp(ms: i64, counter: u32, n: u8) -> Stamp {
        Stamp {
            ms,
            counter,
            device: device(n),
        }
    }

    #[test]
    fn device_ids_are_hex_and_checked() {
        let id = DeviceId::from_bytes([0xab, 1, 2, 3, 4, 5, 6, 0xff]);
        assert_eq!(id.as_str(), "ab010203040506ff");
        assert!(id.is_valid());
        assert!(!DeviceId("../x".into()).is_valid());
        assert!(!DeviceId("AB010203040506FF".into()).is_valid());
    }

    #[test]
    fn stamps_order_by_time_counter_then_device_and_never_go_back() {
        assert!(stamp(1, 0, 9) < stamp(2, 0, 1));
        assert!(stamp(2, 0, 9) < stamp(2, 1, 1));
        assert!(stamp(2, 1, 1) < stamp(2, 1, 2));

        let a = device(1);
        let first = Stamp::next(None, 100, &a);
        assert_eq!(first, stamp(100, 0, 1));
        // Same millisecond, then a clock that went back.
        let second = Stamp::next(Some(&first), 100, &a);
        let third = Stamp::next(Some(&second), 50, &a);
        assert!(first < second && second < third);
        assert_eq!(third, stamp(100, 2, 1));
        // After a stamp seen from another PC in the future.
        let seen = stamp(500, 3, 2);
        assert_eq!(Stamp::next(Some(&seen), 200, &a), stamp(500, 4, 1));
        assert_eq!(Stamp::next(Some(&seen), 900, &a), stamp(900, 0, 1));
    }

    #[test]
    fn versions_keep_a_short_history() {
        let mut v = Version::new(stamp(1, 0, 1), Some("a".into()));
        for i in 2..=12 {
            v = v.replaced(stamp(i, 0, 1), Some(format!("v{i}")));
        }
        assert_eq!(v.history.len(), HISTORY_MAX);
        assert_eq!(v.history[0], stamp(11, 0, 1));
        assert!(v.replaced(stamp(13, 0, 1), None).is_deletion());
    }

    #[test]
    fn merges_by_what_each_side_built_on() {
        let base = Version::new(stamp(1, 0, 1), Some("base".into()));
        let ours = base.replaced(stamp(5, 0, 1), Some("ours".into()));
        let theirs = base.replaced(stamp(3, 0, 2), Some("theirs".into()));

        assert_eq!(merge(None, &base), Merge::Apply);
        assert_eq!(merge(Some(&base), &base), Merge::Ignore);
        // Builds on what we have.
        assert_eq!(merge(Some(&base), &ours), Merge::Apply);
        // We already built on it.
        assert_eq!(merge(Some(&ours), &base), Merge::Ignore);
        // Made apart: the newer stamp wins.
        assert_eq!(
            merge(Some(&ours), &theirs),
            Merge::Conflict {
                incoming_wins: false
            }
        );
        assert_eq!(
            merge(Some(&theirs), &ours),
            Merge::Conflict {
                incoming_wins: true
            }
        );
        // Several edits in a row on the other PC still build on ours.
        let later = ours
            .replaced(stamp(6, 0, 2), Some("x".into()))
            .replaced(stamp(7, 0, 2), Some("y".into()));
        assert_eq!(merge(Some(&ours), &later), Merge::Apply);
    }

    #[test]
    fn deletions_made_apart_agree() {
        let base = Version::new(stamp(1, 0, 1), Some("base".into()));
        let ours = base.replaced(stamp(5, 0, 1), None);
        let theirs = base.replaced(stamp(3, 0, 2), None);
        assert_eq!(merge(Some(&ours), &theirs), Merge::Ignore);
        assert_eq!(merge(Some(&theirs), &ours), Merge::Apply);
        // An edit against a deletion is a conflict: the user decides.
        let edited = base.replaced(stamp(9, 0, 2), Some("edited".into()));
        assert_eq!(
            merge(Some(&ours), &edited),
            Merge::Conflict {
                incoming_wins: true
            }
        );
    }

    #[test]
    fn records_round_trip_as_json() {
        let change = Change {
            kind: ItemKind::Note,
            uid: "n1".into(),
            scope: Scope::Project("abc".into()),
            version: Version::new(stamp(1, 0, 1), Some("{}".into())),
        };
        let batch = Batch {
            device: device(1),
            seq: 3,
            changes: vec![change],
        };
        let json = serde_json::to_string(&batch).unwrap();
        assert!(json.contains(r#""kind":"note""#) && json.contains(r#""project":"abc""#));
        assert_eq!(serde_json::from_str::<Batch>(&json).unwrap(), batch);
        assert!(ItemKind::Skill.asks_on_conflict() && !ItemKind::Theme.asks_on_conflict());
    }

    #[test]
    fn paths_name_devices_and_sequences_only() {
        let d = device(0xab);
        assert_eq!(layout::epoch(2), "vault/epochs/000002.json");
        assert_eq!(
            layout::batch(&d, 42),
            "devices/abababababababab/changes/0000000042.hedy"
        );
        assert_eq!(
            layout::batch_seq(&layout::batch(&d, 42)),
            Some((d.clone(), 42))
        );
        assert_eq!(layout::batch_seq(&layout::snapshot(&d)), None);
        assert_eq!(
            layout::batch_seq("devices/../changes/0000000001.hedy"),
            None
        );
        assert_eq!(
            layout::batch_seq("devices/abababababababab/changes/1.hedy"),
            None
        );
    }

    #[test]
    fn reads_who_owns_a_repo_from_any_git_url() {
        let id = |url: &str| RepoIdentity::from_url(url).map(|r| (r.host, r.owner, r.name));
        let own = |h: &str, o: &str, n: &str| Some((h.into(), o.into(), n.into()));
        assert_eq!(
            id("git@github.com:Tu-Usuario/Tienda.git"),
            own("github.com", "tu-usuario", "tienda")
        );
        assert_eq!(
            id("https://github.com/tu-usuario/tienda"),
            own("github.com", "tu-usuario", "tienda")
        );
        assert_eq!(
            id("https://user@gitlab.com/grupo/sub/app.git/"),
            own("gitlab.com", "grupo/sub", "app")
        );
        assert_eq!(
            id("ssh://git@git.example.com:2222/equipo/api.git"),
            own("git.example.com", "equipo", "api")
        );
        assert_eq!(
            id("git@github-work:empresa-org/app.git"),
            own("github-work", "empresa-org", "app")
        );
        assert_eq!(
            id("https://bitbucket.org/cliente-sa/portal.git"),
            own("bitbucket.org", "cliente-sa", "portal")
        );
        assert_eq!(
            id("ssh://git@codeberg.org/~persona/repo"),
            own("codeberg.org", "persona", "repo")
        );
        for local in [
            "/srv/git/repo.git",
            "C:\\repos\\x",
            "file:///srv/x.git",
            "repo",
            "https://github.com/solo",
        ] {
            assert_eq!(id(local), None, "{local}");
        }
        // The same project through an SSH alias and through HTTPS.
        let alias = RepoIdentity::from_url("git@github-work:Empresa-Org/App.git").unwrap();
        let https = RepoIdentity::from_url("https://github.com/empresa-org/app").unwrap();
        assert_eq!(alias.project_key(), https.project_key());
    }
}
