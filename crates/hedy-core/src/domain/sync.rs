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

use super::team::Seat;

/// Stamps of the versions a change replaced that it carries, newest first:
/// enough to recognise a PC's own edits after a few more, small enough to
/// keep snapshots light.
pub const HISTORY_MAX: usize = 8;

/// The recovery key's member id in key epochs (every other member is a PC).
pub const RECOVERY_MEMBER: &str = "recovery";

/// A member of a key epoch as the engine sees it: a PC or the recovery
/// key, its public keys (as the vault writes them) and, in a team, whose PC
/// it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochMember {
    pub id: String,
    pub public: String,
    pub seat: Option<Seat>,
}

/// What an epoch file says: its number, whether it is a team's vault, its
/// members, and the member (of the epoch before) that signed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochInfo {
    pub number: u32,
    pub team: bool,
    pub members: Vec<EpochMember>,
    pub signer: String,
}

impl EpochMember {
    /// A member of a personal vault: no person, no role.
    pub fn new(id: impl Into<String>, public: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            public: public.into(),
            seat: None,
        }
    }
}

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
        is_hex16(&self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 16 lowercase hex characters: a random id (8 bytes) safe in file names.
pub fn is_hex16(id: &str) -> bool {
    id.len() == 16
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
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
            Some(last) if last.ms >= now_ms => match last.counter.checked_add(1) {
                Some(counter) => Stamp {
                    ms: last.ms,
                    counter,
                    device: device.clone(),
                },
                // The counter ran out: the next millisecond.
                None => Stamp {
                    ms: last.ms + 1,
                    counter: 0,
                    device: device.clone(),
                },
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
    /// A suggested change to a team note (docs/decisions.md §35); only in a
    /// team's repo.
    Suggestion,
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
    /// It only says again what the version it replaced said: a PC taking
    /// over a removed PC's version. An edit built on that version wins
    /// over it, with no conflict.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restates: bool,
}

impl Version {
    /// A first version (nothing before it).
    pub fn new(stamp: Stamp, body: Option<String>) -> Self {
        Self {
            stamp,
            history: Vec::new(),
            body,
            restates: false,
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
            restates: false,
        }
    }

    /// The same content again, answered for at `stamp` (see `restates`).
    pub fn restated(&self, stamp: Stamp) -> Self {
        Self {
            restates: true,
            ..self.replaced(stamp, self.body.clone())
        }
    }

    /// Whether this restates a version `other` was built on.
    fn restates_base_of(&self, other: &Self) -> bool {
        self.restates && self.history.first().is_some_and(|s| other.knows(s))
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
    // A version taken over from a removed PC yields to an edit made on top
    // of that PC's version: the edit is newer in all but the stamp.
    if local.restates_base_of(incoming) {
        return Merge::Apply;
    }
    if incoming.restates_base_of(local) {
        return Merge::Ignore;
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

/// What a note carries between PCs: everything but its local id, its uses
/// (each PC's own) and its project (the change's [`Scope`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteBody {
    pub text: String,
    pub category: super::Category,
    pub author: String,
    pub status: super::NoteStatus,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

/// What a skill carries between PCs (not the task it came from: tasks stay
/// on their PC).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillBody {
    pub name: String,
    pub description: String,
    pub body: String,
    pub status: super::SkillStatus,
    pub author: String,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

/// What a prompt template carries between PCs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateBody {
    pub name: String,
    pub text: String,
    /// The built-in example it edits (or deletes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<String>,
    #[serde(default)]
    pub deleted: bool,
}

/// What a theme carries between PCs: everything but its id, which each PC
/// picks (two PCs may have made different themes with the same name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeBody {
    pub name: String,
    pub base: super::theme::ThemeBase,
    pub colors: std::collections::BTreeMap<String, String>,
    pub origin: super::theme::ThemeOrigin,
}

/// Where a project's memory lives.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MemoryHome {
    /// The user's own: synced to every PC.
    Personal,
    /// A group's (a company, a client, an open-source community), named by
    /// the repo owner: never in the personal sync repo.
    Shared(String),
    /// Neither synced nor shared.
    Local,
    /// A team's (docs/decisions.md §35), by its local id: its notes go to
    /// the team's repo, never to the personal one.
    Team(String),
}

impl MemoryHome {
    /// How it is stored: `personal`, `shared:<owner>`, `local`, `team:<id>`.
    pub fn as_stored(&self) -> String {
        match self {
            Self::Personal => "personal".into(),
            Self::Shared(owner) => format!("shared:{owner}"),
            Self::Local => "local".into(),
            Self::Team(id) => format!("team:{id}"),
        }
    }

    /// Reads [`Self::as_stored`]; anything else is `None` (not decided).
    pub fn from_stored(text: &str) -> Option<Self> {
        match text {
            "personal" => Some(Self::Personal),
            "local" => Some(Self::Local),
            _ => {
                if let Some(id) = text.strip_prefix("team:") {
                    return is_hex16(id).then(|| Self::Team(id.to_owned()));
                }
                text.strip_prefix("shared:")
                    .filter(|owner| !owner.is_empty())
                    .map(|owner| Self::Shared(owner.to_owned()))
            }
        }
    }

    pub fn syncs(&self) -> bool {
        matches!(self, Self::Personal)
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
    /// What its items held before, as digests: a PC joining again with a
    /// row equal to an older version adopts it instead of adding another.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub former: Vec<Former>,
}

/// The digests of the bodies an item held before its newest version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Former {
    pub kind: ItemKind,
    pub uid: String,
    pub bodies: Vec<String>,
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

    /// A PC's batches through `through_seq`, in one file. The number is in
    /// the name so a PC knows whether it needs it without opening it.
    pub fn snapshot(device: &DeviceId, through_seq: u64) -> String {
        format!(
            "{DEVICES_DIR}/{}/snapshot-{through_seq:010}.hedy",
            device.as_str()
        )
    }

    /// Everything a PC wrote lives under this folder.
    pub fn device_dir(device: &DeviceId) -> String {
        format!("{DEVICES_DIR}/{}/", device.as_str())
    }

    /// The PC and `through_seq` of a snapshot path; `None` for any other.
    pub fn snapshot_seq(path: &str) -> Option<(DeviceId, u64)> {
        let rest = path.strip_prefix(DEVICES_DIR)?.strip_prefix('/')?;
        let (device, file) = rest.split_once("/snapshot-")?;
        let seq = file.strip_suffix(".hedy")?;
        let device = DeviceId(device.to_owned());
        if !device.is_valid() || seq.len() != 10 || !seq.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some((device, seq.parse().ok()?))
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

/// What the user said about repo owners, by [`RepoIdentity::owner_key`]
/// (`github.com/tu-usuario`): theirs, or a group's (a company, a client, a
/// community). Each answer covers every repo of that owner on that server;
/// the same name on another server (a company's GitLab) is another owner.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owners {
    #[serde(default)]
    pub mine: std::collections::BTreeSet<String>,
    #[serde(default)]
    pub shared: std::collections::BTreeSet<String>,
}

impl Owners {
    /// Records an answer about `owner` (an owner key), replacing any earlier
    /// one.
    pub fn answer(&mut self, owner: &str, mine: bool) {
        let owner = owner.to_lowercase();
        self.forget(&owner);
        if mine {
            self.mine.insert(owner);
        } else {
            self.shared.insert(owner);
        }
    }

    pub fn forget(&mut self, owner: &str) {
        let owner = owner.to_lowercase();
        self.mine.remove(&owner);
        self.shared.remove(&owner);
    }
}

/// Where a project's memory lives when the user did not choose it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DerivedHome {
    /// From the answers about its owners.
    Known(MemoryHome),
    /// One of its owners is new: ask whether that owner's memory is the
    /// user's alone.
    Ask(String),
    /// No remote, or one that does not say who owns it: it stays here.
    NoRemote,
}

/// Where a project's memory lives, from who owns its repos: every remote
/// counts (`origin`, and the one a fork came from, whatever its name).
/// - any owner the user called shared: the group's;
/// - any owner not answered yet: ask;
/// - all of them the user's: personal.
///
/// A remote that does not say its owner (`None`: a local path, a server
/// without owners) keeps the project here: the safe side.
pub fn derive_home(remotes: &[Option<RepoIdentity>], owners: &Owners) -> DerivedHome {
    let Some(remotes) = remotes.iter().cloned().collect::<Option<Vec<_>>>() else {
        return DerivedHome::NoRemote;
    };
    if remotes.is_empty() {
        return DerivedHome::NoRemote;
    }
    let keys: Vec<String> = remotes.iter().map(RepoIdentity::owner_key).collect();
    if let Some(shared) = keys.iter().find(|k| owners.shared.contains(*k)) {
        return DerivedHome::Known(MemoryHome::Shared(shared.clone()));
    }
    if let Some(new) = keys.iter().find(|k| !owners.mine.contains(*k)) {
        return DerivedHome::Ask(new.clone());
    }
    DerivedHome::Known(MemoryHome::Personal)
}

/// What a synced item says, to show it in a list or side by side: a note's
/// text, a skill's or template's name and text, a theme's name. `None` for
/// a deletion (or a body this version cannot read).
pub fn item_preview(kind: ItemKind, body: Option<&str>) -> Option<String> {
    let body = body?;
    match kind {
        ItemKind::Note => serde_json::from_str::<NoteBody>(body).ok().map(|n| n.text),
        ItemKind::Skill => serde_json::from_str::<SkillBody>(body)
            .ok()
            .map(|s| format!("{}\n\n{}", s.name, s.body)),
        ItemKind::Template => serde_json::from_str::<TemplateBody>(body)
            .ok()
            .filter(|t| !t.deleted)
            .map(|t| format!("{}\n\n{}", t.name, t.text)),
        ItemKind::Theme => serde_json::from_str::<ThemeBody>(body).ok().map(|t| t.name),
        ItemKind::Suggestion => serde_json::from_str::<super::team::SuggestionBody>(body)
            .ok()
            .and_then(|s| s.text),
    }
}

/// A recovery or invite word as compared: lower case, without Spanish
/// accents (the user may type `abaco` for `ábaco`), but with its `ñ`. The
/// BIP39 Spanish list writes accents as separate marks (`a` + `´`); those go
/// too.
pub fn fold_word(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    for c in word.to_lowercase().chars() {
        match c {
            'á' => out.push('a'),
            'é' => out.push('e'),
            'í' => out.push('i'),
            'ó' => out.push('o'),
            'ú' | 'ü' => out.push('u'),
            '\u{303}' if out.ends_with('n') => {
                out.pop();
                out.push('ñ');
            }
            '\u{300}'..='\u{36f}' => {}
            c => out.push(c),
        }
    }
    out
}

/// Whether a git URL carries a password or a token (`https://user:token@…`,
/// `https://token@…`, `…?access_token=…`): Hedy never keeps one (rule 1). A
/// bare SSH user (`git@github.com:…`, `ssh://git@host/…`) is fine.
pub fn url_has_credentials(url: &str) -> bool {
    let mut url = url.trim();
    // git's `<transport>::<address>` form (`https::https://…`).
    while let Some((helper, rest)) = url.split_once("::")
        && !helper.is_empty()
        && helper
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        url = rest;
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        // scp-like: `user@host:path`. Only an `@` before the path names a
        // user, and a `user:password@` there is refused.
        let head = url.split('/').next().unwrap_or("");
        return head
            .rsplit_once('@')
            .is_some_and(|(user, _)| user.contains(':'));
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let userinfo = authority.rsplit_once('@').map(|(userinfo, _)| userinfo);
    match scheme.to_lowercase().as_str() {
        // Over HTTP and FTP a user is a token or comes with one, and a
        // query can carry one too.
        "http" | "https" | "ftp" | "ftps" => userinfo.is_some() || rest.contains('?'),
        _ => userinfo.is_some_and(|userinfo| userinfo.contains(':')),
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

    /// Who owns it, with its server: what an answer about an owner is kept
    /// by (`github.com/tu-usuario` is not `gitlab.empresa.com/tu-usuario`).
    pub fn owner_key(&self) -> String {
        format!("{}/{}", self.host, self.owner)
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
        let full = stamp(700, u32::MAX, 2);
        assert_eq!(Stamp::next(Some(&full), 100, &a), stamp(701, 0, 1));
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
    fn memory_homes_are_stored_as_text() {
        for home in [
            MemoryHome::Personal,
            MemoryHome::Shared("empresa-org".into()),
            MemoryHome::Local,
            MemoryHome::Team("0123456789abcdef".into()),
        ] {
            assert_eq!(MemoryHome::from_stored(&home.as_stored()), Some(home));
        }
        assert_eq!(MemoryHome::from_stored("shared:"), None);
        assert_eq!(MemoryHome::from_stored("team:../x"), None);
        assert!(!MemoryHome::Team("0123456789abcdef".into()).syncs());
        assert_eq!(MemoryHome::from_stored("work"), None);
        assert!(MemoryHome::Personal.syncs() && !MemoryHome::Local.syncs());
    }

    #[test]
    fn derives_where_a_projects_memory_lives_from_its_owners() {
        let repo = |url: &str| RepoIdentity::from_url(url);
        let mut owners = Owners::default();
        let mine = repo("git@github.com:tu-usuario/tienda.git");
        let work = repo("https://github.com/Empresa-Org/app");
        assert_eq!(derive_home(&[], &owners), DerivedHome::NoRemote);
        assert_eq!(
            derive_home(std::slice::from_ref(&work), &owners),
            DerivedHome::Ask("github.com/empresa-org".into())
        );

        owners.answer("github.com/Tu-Usuario", true);
        owners.answer("github.com/empresa-org", false);
        assert_eq!(
            derive_home(std::slice::from_ref(&mine), &owners),
            DerivedHome::Known(MemoryHome::Personal)
        );
        // A fork: any remote of a group makes it the group's, whatever its name.
        let fork = repo("git@github.com:tu-usuario/app.git");
        assert_eq!(
            derive_home(&[fork.clone(), work.clone()], &owners),
            DerivedHome::Known(MemoryHome::Shared("github.com/empresa-org".into()))
        );
        // The same name on another server is another owner.
        let gitlab = repo("git@gitlab.empresa.com:tu-usuario/herramienta.git");
        assert_eq!(
            derive_home(&[gitlab], &owners),
            DerivedHome::Ask("gitlab.empresa.com/tu-usuario".into())
        );
        // A remote that does not say its owner keeps it here.
        assert_eq!(
            derive_home(&[fork.clone(), None], &owners),
            DerivedHome::NoRemote
        );
        // Any owner not answered yet asks, even next to the user's.
        let other = repo("https://github.com/otra-org/app");
        assert_eq!(
            derive_home(&[mine, other], &owners),
            DerivedHome::Ask("github.com/otra-org".into())
        );

        owners.answer("github.com/empresa-org", true);
        assert!(owners.mine.contains("github.com/empresa-org"));
        assert!(!owners.shared.contains("github.com/empresa-org"));
        owners.forget("GITHUB.COM/EMPRESA-ORG");
        assert_eq!(
            derive_home(&[work], &owners),
            DerivedHome::Ask("github.com/empresa-org".into())
        );
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
        assert_eq!(layout::batch_seq(&layout::snapshot(&d, 7)), None);
        assert_eq!(
            layout::snapshot_seq(&layout::snapshot(&d, 7)),
            Some((d.clone(), 7))
        );
        assert_eq!(layout::snapshot_seq(&layout::batch(&d, 1)), None);
        assert_eq!(
            layout::snapshot_seq("devices/../snapshot-0000000007.hedy"),
            None
        );
        assert!(layout::batch(&d, 1).starts_with(&layout::device_dir(&d)));
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
    fn a_taken_over_version_yields_to_an_edit_built_on_the_original() {
        let original = Version::new(stamp(1, 0, 0xaa), Some("uno".into()));
        let taken = original.restated(Stamp {
            device: device(0xbb),
            ..original.stamp.clone()
        });
        // Made on top of the removed PC's version, even if older.
        let edit = original.replaced(stamp(2, 0, 0xcc), Some("dos".into()));
        assert_eq!(merge(Some(&taken), &edit), Merge::Apply);
        assert_eq!(merge(Some(&edit), &taken), Merge::Ignore);
        // An edit made apart from it is still a conflict.
        let apart = Version::new(stamp(3, 0, 0xcc), Some("tres".into()));
        assert!(matches!(
            merge(Some(&taken), &apart),
            Merge::Conflict { .. }
        ));
        // Restating survives a round trip; a plain version does not carry it.
        let text = serde_json::to_string(&taken).unwrap();
        assert_eq!(serde_json::from_str::<Version>(&text).unwrap(), taken);
        assert!(!serde_json::to_string(&edit).unwrap().contains("restates"));
    }

    #[test]
    fn a_preview_says_what_each_item_holds() {
        let template = TemplateBody {
            name: "Revisar".into(),
            text: "Revisa el diff".into(),
            builtin: None,
            deleted: false,
        };
        let body = serde_json::to_string(&template).unwrap();
        assert_eq!(
            item_preview(ItemKind::Template, Some(&body)).as_deref(),
            Some("Revisar\n\nRevisa el diff")
        );
        let deleted = serde_json::to_string(&TemplateBody {
            deleted: true,
            ..template
        })
        .unwrap();
        assert_eq!(item_preview(ItemKind::Template, Some(&deleted)), None);
        assert_eq!(item_preview(ItemKind::Note, None), None);
        assert_eq!(item_preview(ItemKind::Note, Some("{not json")), None);
    }

    #[test]
    fn urls_with_a_password_or_a_token_are_told_apart() {
        for bad in [
            "https://alice:ghp_TOKEN@github.com/a/b.git",
            "https://ghp_TOKEN@github.com/a/b.git",
            "http://user@host/a.git",
            "ssh://git:secret@host/a.git",
            "git:secret@github.com:a/b.git",
            "https::https://ghp_TOKEN@github.com/a/b.git",
            "HTTPS://alice:tok@github.com",
            "https://github.com/a/b.git?access_token=ghp_TOKEN",
            "ftps://tok@host/a.git",
        ] {
            assert!(url_has_credentials(bad), "{bad}");
        }
        for good in [
            "https://github.com/a/b.git",
            "git@github.com:a/b.git",
            "ssh://git@host:2222/a/b.git",
            "/srv/git/b.git",
            "https://github.com/a/b@v1",
            "github.com:org/repo@v1.git",
            "ext::ssh -i key git@host %S a.git",
            "ssh::git@github.com:a/b.git",
        ] {
            assert!(!url_has_credentials(good), "{good}");
        }
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
