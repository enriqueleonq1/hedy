//! The sync vault's cryptography (docs/decisions.md §34): everything that
//! leaves this PC for the sync repo is sealed here, with well-known
//! constructions from the RustCrypto and dalek projects, nothing home-made:
//!
//! - **Data**: XChaCha20-Poly1305 under a key derived (HKDF-SHA256) from the
//!   vault key. Each file is bound to its path and key epoch (the AEAD's
//!   associated data), so a file moved, swapped or altered does not open.
//! - **The vault key** is random and changes when a PC is removed (a new
//!   epoch). Each epoch file wraps it for every member with a sealed box
//!   (X25519 + XSalsa20-Poly1305, libsodium's `crypto_box_seal`).
//! - **Members** are the PCs and the recovery key; each has a box key pair
//!   and an Ed25519 signing key pair. A PC's secrets live in the OS keyring.
//! - **The recovery key** comes from 12 words (BIP39, 128 bits): its keys
//!   are derived from them (HKDF over the BIP39 seed), never stored.
//! - **Trust**: epoch 1 is signed by the recovery key, and every later one
//!   by a member of the one before. A PC removed from an epoch cannot sign
//!   the next, so it cannot put its own key back, even with push access.
//! - **Teams** (§35): each PC also names its person and their role. An
//!   admin's PC adds, removes and changes anyone; a member's PC only adds or
//!   removes that person's own PCs. Personal vaults have no seats, and there
//!   a PC only removes.

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use bip39::{Language, Mnemonic};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroize;

use crate::application::ports::{PortError, PortResult, SyncCrypto};
use crate::domain::sync::{DeviceId, EpochMember, fold_word};
use crate::domain::team::{PersonId, Role, Seat};
use crate::tr;

/// The member id of the recovery key in epoch files.
pub const RECOVERY_ID: &str = crate::domain::sync::RECOVERY_MEMBER;
/// Version of the epoch and sealed-file formats.
const FORMAT: u32 = 1;
/// Sealed files start with this, then the format byte and the epoch.
const MAGIC: &[u8; 4] = b"HEDY";
const HEADER_LEN: usize = MAGIC.len() + 1 + 4;
const NONCE_LEN: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    /// The system's random source failed.
    Random,
    /// Not a sealed file, cut short, or altered.
    Corrupt,
    /// Sealed for another key: not a member, or an epoch this PC lacks.
    NotForUs,
    /// The words are not a recovery key.
    BadPhrase,
    /// An epoch file that this PC must not trust.
    Untrusted(Distrust),
}

/// Why an epoch file is not trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distrust {
    /// No epochs at all.
    Empty,
    /// The first epoch is not epoch 1 of a known format.
    FirstEpoch,
    /// The first epoch is not signed by this recovery key.
    Recovery,
    /// Not the epoch right after the trusted one.
    Sequence,
    /// Signed by someone who was not a member of the epoch before.
    Signer,
    /// The signature does not match.
    Signature,
    /// A member id that is not a PC's, a member twice, a seat that does
    /// not hold together, or a change its signer may not make (a personal
    /// PC only removes; a team member only adds or removes their own PCs).
    Members,
}

impl fmt::Display for Distrust {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self {
            Self::Empty => tr!("no hay épocas", "there are no epochs"),
            Self::FirstEpoch => tr!(
                "la primera época no es válida",
                "the first epoch is not valid"
            ),
            Self::Recovery => tr!(
                "no la firmó tu clave de recuperación",
                "your recovery key did not sign it"
            ),
            Self::Sequence => tr!("se saltó una época", "an epoch was skipped"),
            Self::Signer => tr!(
                "la firmó una PC que ya no era miembro",
                "a PC that was no longer a member signed it"
            ),
            Self::Signature => tr!("la firma no coincide", "the signature does not match"),
            Self::Members => tr!(
                "sus miembros no son válidos, o quien la firmó no podía hacer ese cambio",
                "its members are not valid, or its signer could not make that change"
            ),
        })
    }
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self {
            Self::Random => tr!(
                "el sistema no dio números aleatorios",
                "the system gave no random numbers"
            ),
            Self::Corrupt => tr!(
                "un archivo de la sincronización está dañado o fue alterado",
                "a sync file is damaged or was altered"
            ),
            Self::NotForUs => tr!(
                "esta PC no tiene la clave de ese archivo",
                "this PC does not have that file's key"
            ),
            Self::BadPhrase => tr!(
                "esas palabras no son una clave de recuperación",
                "those words are not a recovery key"
            ),
            Self::Untrusted(why) => tr!(
                "el cambio de claves no es de confianza ({why})",
                "the key change is not trusted ({why})"
            ),
        })
    }
}

impl std::error::Error for VaultError {}

type Result<T> = std::result::Result<T, VaultError>;

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|_| VaultError::Random)?;
    Ok(bytes)
}

/// 32 secret bytes, wiped when dropped and never printed.
#[derive(Clone, PartialEq, Eq)]
struct Secret([u8; 32]);

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(…)")
    }
}

fn hkdf32(salt: &[u8], ikm: &[u8], info: &[u8]) -> Secret {
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, &mut out)
        .expect("32 bytes is a valid HKDF-SHA256 length");
    Secret(out)
}

/// A member's public keys, as stored in epoch files (base64).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberPublic {
    pub box_key: String,
    pub sign_key: String,
}

impl MemberPublic {
    fn box_key(&self) -> Result<crypto_box::PublicKey> {
        Ok(crypto_box::PublicKey::from_bytes(decode32(&self.box_key)?))
    }

    fn sign_key(&self) -> Result<VerifyingKey> {
        VerifyingKey::from_bytes(&decode32(&self.sign_key)?).map_err(|_| VaultError::Corrupt)
    }
}

fn decode32(text: &str) -> Result<[u8; 32]> {
    BASE64
        .decode(text)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or(VaultError::Corrupt)
}

/// A member's secret keys: a PC's (kept in the keyring) or the recovery
/// key's (derived from the words when they are typed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberSecrets {
    box_key: Secret,
    sign_key: Secret,
}

impl MemberSecrets {
    /// New keys for this PC.
    pub fn generate() -> Result<Self> {
        Ok(Self {
            box_key: Secret(random()?),
            sign_key: Secret(random()?),
        })
    }

    /// For the keyring: one base64 string. [`Self::decode`] reads it back.
    pub fn encode(&self) -> String {
        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(&self.box_key.0);
        bytes[32..].copy_from_slice(&self.sign_key.0);
        let text = BASE64.encode(bytes);
        bytes.zeroize();
        text
    }

    pub fn decode(text: &str) -> Result<Self> {
        let mut bytes: [u8; 64] = BASE64
            .decode(text.trim())
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or(VaultError::Corrupt)?;
        let keys = Self {
            box_key: Secret(bytes[..32].try_into().expect("32 bytes")),
            sign_key: Secret(bytes[32..].try_into().expect("32 bytes")),
        };
        bytes.zeroize();
        Ok(keys)
    }

    pub fn public(&self) -> MemberPublic {
        let box_public = crypto_box::SecretKey::from_bytes(self.box_key.0).public_key();
        let sign_public = SigningKey::from_bytes(&self.sign_key.0).verifying_key();
        MemberPublic {
            box_key: BASE64.encode(box_public.as_bytes()),
            sign_key: BASE64.encode(sign_public.to_bytes()),
        }
    }

    fn sign(&self, message: &[u8]) -> String {
        let signature = SigningKey::from_bytes(&self.sign_key.0).sign(message);
        BASE64.encode(signature.to_bytes())
    }

    fn unseal(&self, sealed: &[u8]) -> Result<Vec<u8>> {
        crypto_box::SecretKey::from_bytes(self.box_key.0)
            .unseal(sealed)
            .map_err(|_| VaultError::NotForUs)
    }
}

/// A new recovery key: the 12 words to show the user once, and the keys
/// they stand for. The words are in the interface's language; any BIP39
/// language is read back.
pub fn new_recovery() -> Result<(String, MemberSecrets)> {
    let language = match crate::i18n::language() {
        crate::i18n::Language::Spanish => Language::Spanish,
        _ => Language::English,
    };
    let mut entropy: [u8; 16] = random()?;
    let mnemonic = Mnemonic::from_entropy_in(language, &entropy).map_err(|_| VaultError::Random);
    entropy.zeroize();
    let words = mnemonic?.to_string();
    let keys = recover(&words)?;
    Ok((words, keys))
}

/// The recovery key's secrets from its 12 words, as the user typed them:
/// any case, extra spaces, numbers in front, and Spanish words without
/// their accents.
pub fn recover(words: &str) -> Result<MemberSecrets> {
    let typed: Vec<String> = words
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')'))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let mnemonic = Mnemonic::parse(typed.join(" "))
        .or_else(|_| Mnemonic::parse(with_accents(&typed, Language::Spanish).join(" ")))
        .map_err(|_| VaultError::BadPhrase)?;
    if mnemonic.word_count() != 12 {
        return Err(VaultError::BadPhrase);
    }
    let mut seed = mnemonic.to_seed("");
    let keys = MemberSecrets {
        box_key: hkdf32(b"hedy-sync-recovery-v1", &seed, b"box"),
        sign_key: hkdf32(b"hedy-sync-recovery-v1", &seed, b"sign"),
    };
    seed.zeroize();
    Ok(keys)
}

/// Each word as written in `language`'s list when it only lacks accents.
fn with_accents(words: &[String], language: Language) -> Vec<String> {
    words
        .iter()
        .map(|w| {
            language
                .word_list()
                .iter()
                .find(|listed| fold_word(listed) == fold_word(w))
                .map_or_else(|| w.clone(), |listed| (*listed).to_owned())
        })
        .collect()
}

/// The key of one epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultKey {
    epoch: u32,
    key: Secret,
}

impl VaultKey {
    pub fn generate(epoch: u32) -> Result<Self> {
        Ok(Self {
            epoch,
            key: Secret(random()?),
        })
    }

    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    /// For the keyring: `<epoch>:<base64>`. [`Self::decode`] reads it back.
    pub fn encode(&self) -> String {
        format!("{}:{}", self.epoch, BASE64.encode(self.key.0))
    }

    pub fn decode(text: &str) -> Result<Self> {
        let (epoch, key) = text.split_once(':').ok_or(VaultError::Corrupt)?;
        Ok(Self {
            epoch: epoch.parse().map_err(|_| VaultError::Corrupt)?,
            key: Secret(decode32(key)?),
        })
    }

    fn subkey(&self, purpose: &[u8]) -> Secret {
        hkdf32(b"hedy-sync-vault-v1", &self.key.0, purpose)
    }

    /// What names a project in the repo: an HMAC of its
    /// `RepoIdentity::project_key`, so the same project matches on every PC
    /// and the repo never shows its name.
    pub fn project_id(&self, project_key: &str) -> String {
        let key = self.subkey(b"project-id");
        let mut mac =
            <Hmac<Sha256> as hmac::KeyInit>::new_from_slice(&key.0).expect("any key size");
        mac.update(project_key.as_bytes());
        mac.finalize().into_bytes()[..16]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// Seals `plaintext` for the file at `path` in the repo.
    pub fn seal(&self, path: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let header = header(self.epoch);
        let nonce: [u8; NONCE_LEN] = random()?;
        let aad = [header.as_slice(), path.as_bytes()].concat();
        let cipher =
            XChaCha20Poly1305::new_from_slice(&self.subkey(b"data").0).expect("32-byte key");
        let sealed = cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| VaultError::Corrupt)?;
        Ok([header.as_slice(), &nonce, &sealed].concat())
    }
}

fn header(epoch: u32) -> [u8; HEADER_LEN] {
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4] = FORMAT as u8;
    header[5..].copy_from_slice(&epoch.to_be_bytes());
    header
}

/// The epoch a sealed file was sealed with, to pick its key.
pub fn sealed_epoch(sealed: &[u8]) -> Result<u32> {
    if sealed.len() < HEADER_LEN + NONCE_LEN || &sealed[..4] != MAGIC || sealed[4] != FORMAT as u8 {
        return Err(VaultError::Corrupt);
    }
    Ok(u32::from_be_bytes(
        sealed[5..9].try_into().expect("4 bytes"),
    ))
}

/// Opens a file sealed with [`VaultKey::seal`] for `path`, with the key of
/// its epoch from `keys`.
pub fn open(keys: &[VaultKey], path: &str, sealed: &[u8]) -> Result<Vec<u8>> {
    let epoch = sealed_epoch(sealed)?;
    let key = keys
        .iter()
        .find(|k| k.epoch == epoch)
        .ok_or(VaultError::NotForUs)?;
    let (header, rest) = sealed.split_at(HEADER_LEN);
    let (nonce, body) = rest.split_at(NONCE_LEN);
    let aad = [header, path.as_bytes()].concat();
    let cipher = XChaCha20Poly1305::new_from_slice(&key.subkey(b"data").0).expect("32-byte key");
    cipher
        .decrypt(
            &XNonce::from(<[u8; NONCE_LEN]>::try_from(nonce).expect("24 bytes")),
            Payload {
                msg: body,
                aad: &aad,
            },
        )
        .map_err(|_| VaultError::Corrupt)
}

/// One member of an epoch: its public keys and the vault key sealed for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// A device id, or [`RECOVERY_ID`].
    pub id: String,
    /// In a team, whose PC it is and their role; never in a personal vault,
    /// whose files (and signatures) stay as they were.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<PersonId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub public: MemberPublic,
    pub wrapped_key: String,
}

impl Member {
    fn seat(&self) -> Option<Seat> {
        Some(Seat {
            person: self.person.clone()?,
            role: self.role?,
        })
    }
}

/// `vault/epochs/<n>.json`: who can read epoch `n` and who signed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochFile {
    pub format: u32,
    pub epoch: u32,
    pub members: Vec<Member>,
    /// The member that signed it (in the previous epoch, or the recovery key
    /// for the first one).
    pub signer: String,
    #[serde(default)]
    pub signature: String,
}

impl EpochFile {
    /// `key` sealed for each of `members` (id, public keys) of a personal
    /// vault, signed by `signer`.
    pub fn create(
        key: &VaultKey,
        members: &[(String, MemberPublic)],
        signer_id: &str,
        signer: &MemberSecrets,
    ) -> Result<Self> {
        let members: Vec<_> = members
            .iter()
            .map(|(id, public)| (id.clone(), public.clone(), None))
            .collect();
        Self::create_seated(key, &members, signer_id, signer)
    }

    /// [`Self::create`] where each member may have a team seat.
    pub fn create_seated(
        key: &VaultKey,
        members: &[(String, MemberPublic, Option<Seat>)],
        signer_id: &str,
        signer: &MemberSecrets,
    ) -> Result<Self> {
        let payload = wrap_payload(key);
        let members = members
            .iter()
            .map(|(id, public, seat)| {
                let sealed = public
                    .box_key()?
                    .seal(&mut crypto_box::aead::OsRng, &payload)
                    .map_err(|_| VaultError::Random)?;
                Ok(Member {
                    id: id.clone(),
                    person: seat.as_ref().map(|s| s.person.clone()),
                    role: seat.as_ref().map(|s| s.role),
                    public: public.clone(),
                    wrapped_key: BASE64.encode(sealed),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut file = Self {
            format: FORMAT,
            epoch: key.epoch,
            members,
            signer: signer_id.to_owned(),
            signature: String::new(),
        };
        file.signature = signer.sign(&file.signed_bytes());
        Ok(file)
    }

    /// What the signature covers: the file without its signature.
    fn signed_bytes(&self) -> Vec<u8> {
        let unsigned = Self {
            signature: String::new(),
            ..self.clone()
        };
        serde_json::to_vec(&unsigned).expect("an epoch file serializes")
    }

    pub fn member(&self, id: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.id == id)
    }

    /// The vault key, for member `id` holding `secrets`.
    pub fn open(&self, id: &str, secrets: &MemberSecrets) -> Result<VaultKey> {
        let member = self.member(id).ok_or(VaultError::NotForUs)?;
        if member.public != secrets.public() {
            return Err(VaultError::NotForUs);
        }
        let sealed = BASE64
            .decode(&member.wrapped_key)
            .map_err(|_| VaultError::Corrupt)?;
        let mut payload = secrets.unseal(&sealed)?;
        let key = unwrap_payload(&payload, self.epoch);
        payload.zeroize();
        key
    }

    fn signed_by(&self, public: &MemberPublic) -> Result<()> {
        let bytes: [u8; 64] = BASE64
            .decode(&self.signature)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or(VaultError::Untrusted(Distrust::Signature))?;
        public
            .sign_key()?
            .verify_strict(&self.signed_bytes(), &Signature::from_bytes(&bytes))
            .map_err(|_| VaultError::Untrusted(Distrust::Signature))
    }

    /// Whether this is a trustworthy first epoch for the recovery key whose
    /// public keys are `recovery`: epoch 1, with the recovery key as a
    /// member, signed by it.
    pub fn verify_first(&self, recovery: &MemberPublic) -> Result<()> {
        if self.format != FORMAT || self.epoch != 1 {
            return Err(VaultError::Untrusted(Distrust::FirstEpoch));
        }
        if self.signer != RECOVERY_ID
            || self.member(RECOVERY_ID).map(|m| &m.public) != Some(recovery)
        {
            return Err(VaultError::Untrusted(Distrust::Recovery));
        }
        self.signed_by(recovery)?;
        self.valid_members()
    }

    /// Each member once, and every one the recovery key (with no seat) or a
    /// PC id. Seats hold together: every PC has one (a team) or none does
    /// (a personal vault), each with a person id, and all of a person's PCs
    /// have the same role.
    fn valid_members(&self) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        let mut roles = std::collections::HashMap::new();
        let pcs = self.members.iter().filter(|m| m.id != RECOVERY_ID);
        let seated = pcs.clone().filter(|m| m.seat().is_some()).count();
        let valid = (seated == 0 || seated == pcs.count())
            && self.members.iter().all(|m| {
                let seat_ok = if m.id == RECOVERY_ID {
                    m.person.is_none() && m.role.is_none()
                } else {
                    DeviceId(m.id.clone()).is_valid()
                        && m.person.is_some() == m.role.is_some()
                        && m.seat().is_none_or(|s| {
                            s.person.is_valid()
                                && *roles.entry(s.person).or_insert(s.role) == s.role
                        })
                };
                seen.insert(m.id.as_str()) && seat_ok
            });
        if valid {
            Ok(())
        } else {
            Err(VaultError::Untrusted(Distrust::Members))
        }
    }

    /// Whether `next` may follow `self`: the next epoch, signed by a member
    /// of this one, keeping the recovery key. The recovery key may change
    /// anything else. Otherwise every member kept keeps its keys and person,
    /// and:
    /// - a personal PC only removes (a PC joins with the 12 words);
    /// - a team admin's PC adds, removes and changes roles;
    /// - a team member's PC only adds or removes that person's own PCs,
    ///   with their role.
    pub fn verify_next(&self, next: &EpochFile) -> Result<()> {
        if next.format != FORMAT || Some(next.epoch) != self.epoch.checked_add(1) {
            return Err(VaultError::Untrusted(Distrust::Sequence));
        }
        let signer = self
            .member(&next.signer)
            .ok_or(VaultError::Untrusted(Distrust::Signer))?;
        next.signed_by(&signer.public)?;
        next.valid_members()?;
        // The recovery key always stays, unchanged: it is how a PC joins.
        let keeps_recovery = self.member(RECOVERY_ID).map(|m| &m.public)
            == next.member(RECOVERY_ID).map(|m| &m.public);
        if !keeps_recovery {
            return Err(VaultError::Untrusted(Distrust::Members));
        }
        if next.signer == RECOVERY_ID || signer_may_change(signer.seat(), self, next) {
            Ok(())
        } else {
            Err(VaultError::Untrusted(Distrust::Members))
        }
    }
}

/// Whether a PC with `seat` (none: a personal PC) may turn `old`'s members
/// into `new`'s. See [`EpochFile::verify_next`].
fn signer_may_change(seat: Option<Seat>, old: &EpochFile, new: &EpochFile) -> bool {
    let admin = seat.as_ref().is_some_and(|s| s.role == Role::Admin);
    // Whether the signer may add or remove a PC of `of`'s person.
    let theirs = |of: Option<Seat>| {
        admin || matches!((&seat, of), (Some(mine), Some(of)) if mine.person == of.person)
    };
    let kept_or_added = new.members.iter().all(|m| match old.member(&m.id) {
        Some(was) => {
            was.public == m.public && was.person == m.person && (admin || was.role == m.role)
        }
        // A personal PC never adds; a member adds a PC of their own, as
        // what they are.
        None => {
            seat.is_some() && theirs(m.seat()) && (admin || m.role == seat.as_ref().map(|s| s.role))
        }
    });
    let removed = old
        .members
        .iter()
        .filter(|m| new.member(&m.id).is_none())
        .all(|m| seat.is_none() || theirs(m.seat()));
    kept_or_added && removed
}

/// Checks a whole chain of epochs, oldest first, from the recovery key:
/// what a PC joining with the 12 words does. Returns the newest.
pub fn verify_chain<'a>(epochs: &'a [EpochFile], recovery: &MemberPublic) -> Result<&'a EpochFile> {
    let first = epochs
        .first()
        .ok_or(VaultError::Untrusted(Distrust::Empty))?;
    first.verify_first(recovery)?;
    for pair in epochs.windows(2) {
        pair[0].verify_next(&pair[1])?;
    }
    Ok(epochs.last().expect("not empty"))
}

/// What a member's sealed box holds: the epoch, then the key, so a wrapped
/// key copied into another epoch's file does not open there.
fn wrap_payload(key: &VaultKey) -> Vec<u8> {
    [key.epoch.to_be_bytes().as_slice(), &key.key.0].concat()
}

fn unwrap_payload(payload: &[u8], epoch: u32) -> Result<VaultKey> {
    if payload.len() != 36 || payload[..4] != epoch.to_be_bytes() {
        return Err(VaultError::Corrupt);
    }
    Ok(VaultKey {
        epoch,
        key: Secret(payload[4..].try_into().expect("32 bytes")),
    })
}

/// [`SyncCrypto`] over this module:
/// keys, secrets and epoch files travel as text.
#[derive(Default)]
pub struct VaultCrypto;

fn port(error: VaultError) -> PortError {
    PortError::new(error.to_string())
}

fn epoch_file(text: &str) -> Result<EpochFile> {
    serde_json::from_str(text).map_err(|_| VaultError::Corrupt)
}

fn member_public(text: &str) -> Result<MemberPublic> {
    serde_json::from_str(text).map_err(|_| VaultError::Corrupt)
}

impl SyncCrypto for VaultCrypto {
    fn random_hex(&self, bytes: usize) -> PortResult<String> {
        let mut buf = vec![0u8; bytes];
        getrandom::fill(&mut buf).map_err(|_| port(VaultError::Random))?;
        Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
    }

    fn new_member(&self) -> PortResult<String> {
        Ok(MemberSecrets::generate().map_err(port)?.encode())
    }

    fn new_recovery(&self) -> PortResult<(String, String)> {
        let (words, keys) = new_recovery().map_err(port)?;
        Ok((words, keys.encode()))
    }

    fn recover(&self, words: &str) -> PortResult<String> {
        Ok(recover(words).map_err(port)?.encode())
    }

    fn member_public(&self, secrets: &str) -> PortResult<String> {
        let public = MemberSecrets::decode(secrets).map_err(port)?.public();
        Ok(serde_json::to_string(&public).expect("public keys serialize"))
    }

    fn new_epoch(
        &self,
        epoch: u32,
        members: &[EpochMember],
        signer_id: &str,
        signer: &str,
    ) -> PortResult<(String, String)> {
        let key = VaultKey::generate(epoch).map_err(port)?;
        let members = members
            .iter()
            .map(|m| Ok((m.id.clone(), member_public(&m.public)?, m.seat.clone())))
            .collect::<Result<Vec<_>>>()
            .map_err(port)?;
        let signer = MemberSecrets::decode(signer).map_err(port)?;
        let file = EpochFile::create_seated(&key, &members, signer_id, &signer).map_err(port)?;
        let text = serde_json::to_string_pretty(&file).expect("an epoch file serializes");
        Ok((text, key.encode()))
    }

    fn verify_first_epoch(&self, file: &str, recovery: &str) -> PortResult<()> {
        epoch_file(file)
            .and_then(|f| f.verify_first(&member_public(recovery)?))
            .map_err(port)
    }

    fn verify_next_epoch(&self, previous: &str, next: &str) -> PortResult<()> {
        epoch_file(previous)
            .and_then(|p| p.verify_next(&epoch_file(next)?))
            .map_err(port)
    }

    fn epoch_members(&self, file: &str) -> PortResult<(u32, Vec<EpochMember>)> {
        let file = epoch_file(file).map_err(port)?;
        let members = file
            .members
            .iter()
            .map(|m| EpochMember {
                id: m.id.clone(),
                public: serde_json::to_string(&m.public).expect("public keys serialize"),
                seat: m.seat(),
            })
            .collect();
        Ok((file.epoch, members))
    }

    fn open_epoch(&self, file: &str, id: &str, secrets: &str) -> PortResult<String> {
        let secrets = MemberSecrets::decode(secrets).map_err(port)?;
        let key = epoch_file(file)
            .and_then(|f| f.open(id, &secrets))
            .map_err(port)?;
        Ok(key.encode())
    }

    fn seal(&self, key: &str, path: &str, plaintext: &[u8]) -> PortResult<Vec<u8>> {
        VaultKey::decode(key)
            .and_then(|k| k.seal(path, plaintext))
            .map_err(port)
    }

    fn open(&self, keys: &[String], path: &str, sealed: &[u8]) -> PortResult<Vec<u8>> {
        let keys = keys
            .iter()
            .map(|k| VaultKey::decode(k))
            .collect::<Result<Vec<_>>>()
            .map_err(port)?;
        open(&keys, path, sealed).map_err(port)
    }

    fn project_id(&self, key: &str, project_key: &str) -> PortResult<String> {
        Ok(VaultKey::decode(key).map_err(port)?.project_id(project_key))
    }

    fn digest(&self, bytes: &[u8]) -> String {
        use sha2::Digest;
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn sign(&self, secrets: &str, bytes: &[u8]) -> PortResult<String> {
        Ok(MemberSecrets::decode(secrets).map_err(port)?.sign(bytes))
    }

    fn verify(&self, public: &str, bytes: &[u8], signature: &str) -> PortResult<()> {
        let signature: [u8; 64] = BASE64
            .decode(signature)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| port(VaultError::Untrusted(Distrust::Signature)))?;
        member_public(public)
            .and_then(|p| p.sign_key())
            .and_then(|key| {
                key.verify_strict(bytes, &Signature::from_bytes(&signature))
                    .map_err(|_| VaultError::Untrusted(Distrust::Signature))
            })
            .map_err(port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language as UiLanguage, with_language};

    fn pc() -> (MemberSecrets, MemberPublic) {
        let secrets = MemberSecrets::generate().unwrap();
        let public = secrets.public();
        (secrets, public)
    }

    #[test]
    fn sealed_files_open_only_at_their_path_with_their_key() {
        let key = VaultKey::generate(1).unwrap();
        let path = "devices/abababababababab/changes/0000000001.hedy";
        let sealed = key.seal(path, b"notas").unwrap();
        assert_eq!(sealed_epoch(&sealed).unwrap(), 1);
        assert_eq!(
            open(std::slice::from_ref(&key), path, &sealed).unwrap(),
            b"notas"
        );
        // The same text sealed twice looks different.
        assert_ne!(key.seal(path, b"notas").unwrap(), sealed);

        // Moved to another path, altered, or opened with another key.
        let other = "devices/abababababababab/changes/0000000002.hedy";
        assert_eq!(
            open(std::slice::from_ref(&key), other, &sealed),
            Err(VaultError::Corrupt)
        );
        let mut altered = sealed.clone();
        *altered.last_mut().unwrap() ^= 1;
        assert_eq!(
            open(std::slice::from_ref(&key), path, &altered),
            Err(VaultError::Corrupt)
        );
        let stranger = VaultKey::generate(1).unwrap();
        assert_eq!(open(&[stranger], path, &sealed), Err(VaultError::Corrupt));
        assert_eq!(
            open(&[VaultKey::generate(2).unwrap()], path, &sealed),
            Err(VaultError::NotForUs)
        );
        assert_eq!(open(&[key], path, b"HEDY"), Err(VaultError::Corrupt));
    }

    #[test]
    fn project_ids_are_stable_per_key_and_hide_the_name() {
        let key = VaultKey::generate(1).unwrap();
        let id = key.project_id("tu-usuario/tienda");
        assert_eq!(id.len(), 32);
        assert_eq!(id, key.project_id("tu-usuario/tienda"));
        assert!(!id.contains("tienda"));
        assert_ne!(id, key.project_id("tu-usuario/recetas"));
        assert_ne!(
            id,
            VaultKey::generate(1)
                .unwrap()
                .project_id("tu-usuario/tienda")
        );
    }

    #[test]
    fn pc_secrets_round_trip_through_the_keyring_text() {
        let (secrets, public) = pc();
        let text = secrets.encode();
        let back = MemberSecrets::decode(&text).unwrap();
        assert_eq!(back.public(), public);
        assert!(!format!("{back:?}").contains(&text));
        assert_eq!(MemberSecrets::decode("corto"), Err(VaultError::Corrupt));
    }

    #[test]
    fn recovery_words_give_the_same_keys_however_they_are_typed() {
        let (words, keys) = with_language(UiLanguage::Spanish, || new_recovery().unwrap());
        assert_eq!(words.split(' ').count(), 12);
        assert_eq!(recover(&words).unwrap().public(), keys.public());

        // Numbered, upper case, extra spaces and no accents.
        let messy: String = words
            .split(' ')
            .enumerate()
            .map(|(i, w)| format!("{}. {}  ", i + 1, w.to_uppercase()))
            .collect::<String>()
            .replace(['Á', 'á'], "a")
            .replace(['É', 'é'], "e")
            .replace(['Í', 'í'], "i")
            .replace(['Ó', 'ó'], "o")
            .replace(['Ú', 'ú'], "u");
        assert_eq!(recover(&messy).unwrap().public(), keys.public());

        let english = with_language(UiLanguage::English, || new_recovery().unwrap()).0;
        assert!(recover(&english).is_ok());
        assert_eq!(recover("hola mundo").unwrap_err(), VaultError::BadPhrase);
        let mut wrong: Vec<&str> = words.split(' ').collect();
        wrong.swap(0, 1);
        // A swapped word fails the checksum (almost always).
        assert!(recover(&wrong.join(" ")).map(|k| k.public()) != Ok(keys.public()));
    }

    #[test]
    fn a_pc_only_removes_members_and_ids_are_pcs() {
        const HOME: &str = "00000000000000aa";
        const OFFICE: &str = "00000000000000bb";
        let (_, recovery) = with_language(UiLanguage::English, || new_recovery().unwrap());
        let (home, home_public) = pc();
        let (_, office_public) = pc();
        let (_, phantom_public) = pc();
        let members = |list: &[(&str, &MemberPublic)]| -> Vec<(String, MemberPublic)> {
            list.iter()
                .map(|(id, p)| ((*id).to_owned(), (*p).clone()))
                .collect()
        };
        let e1 = EpochFile::create(
            &VaultKey::generate(1).unwrap(),
            &members(&[
                (RECOVERY_ID, &recovery.public()),
                (HOME, &home_public),
                (OFFICE, &office_public),
            ]),
            RECOVERY_ID,
            &recovery,
        )
        .unwrap();
        let next = |list: &[(&str, &MemberPublic)]| {
            EpochFile::create(&VaultKey::generate(2).unwrap(), &members(list), HOME, &home).unwrap()
        };
        let refused = Err(VaultError::Untrusted(Distrust::Members));
        // Adding a member, or changing another's keys.
        let phantom = "00000000000000cc";
        assert_eq!(
            e1.verify_next(&next(&[
                (RECOVERY_ID, &recovery.public()),
                (HOME, &home_public),
                (OFFICE, &office_public),
                (phantom, &phantom_public),
            ])),
            refused
        );
        assert_eq!(
            e1.verify_next(&next(&[
                (RECOVERY_ID, &recovery.public()),
                (HOME, &home_public),
                (OFFICE, &phantom_public),
            ])),
            refused
        );
        // An id that is not a PC's (`devices//` would be every PC's folder).
        assert_eq!(
            e1.verify_next(&next(&[
                (RECOVERY_ID, &recovery.public()),
                ("", &home_public)
            ])),
            refused
        );
        // Removing is fine.
        e1.verify_next(&next(&[
            (RECOVERY_ID, &recovery.public()),
            (HOME, &home_public),
        ]))
        .unwrap();
    }

    #[test]
    fn epochs_let_members_in_and_removed_pcs_out() {
        const HOME: &str = "00000000000000aa";
        const OFFICE: &str = "00000000000000bb";
        let (_, recovery) = with_language(UiLanguage::English, || new_recovery().unwrap());
        let recovery_public = recovery.public();
        let (home, home_public) = pc();
        let (office, office_public) = pc();

        // Setup on HOME: signed by the recovery key.
        let k1 = VaultKey::generate(1).unwrap();
        let e1 = EpochFile::create(
            &k1,
            &[
                (RECOVERY_ID.into(), recovery_public.clone()),
                (HOME.to_owned(), home_public.clone()),
            ],
            RECOVERY_ID,
            &recovery,
        )
        .unwrap();
        e1.verify_first(&recovery_public).unwrap();
        assert_eq!(e1.open(HOME, &home).unwrap(), k1);
        assert_eq!(e1.open(RECOVERY_ID, &recovery).unwrap(), k1);
        assert_eq!(e1.open(OFFICE, &office), Err(VaultError::NotForUs));
        assert_eq!(e1.open(HOME, &office), Err(VaultError::NotForUs));

        // OFFICE joins with the words: the recovery key signs epoch 2.
        let k2 = VaultKey::generate(2).unwrap();
        let e2 = EpochFile::create(
            &k2,
            &[
                (RECOVERY_ID.into(), recovery_public.clone()),
                (HOME.to_owned(), home_public.clone()),
                (OFFICE.to_owned(), office_public.clone()),
            ],
            RECOVERY_ID,
            &recovery,
        )
        .unwrap();
        e1.verify_next(&e2).unwrap();
        assert_eq!(e2.open(OFFICE, &office).unwrap(), k2);

        // HOME removes OFFICE: epoch 3, signed by home.
        let k3 = VaultKey::generate(3).unwrap();
        let e3 = EpochFile::create(
            &k3,
            &[
                (RECOVERY_ID.into(), recovery_public.clone()),
                (HOME.to_owned(), home_public.clone()),
            ],
            HOME,
            &home,
        )
        .unwrap();
        e2.verify_next(&e3).unwrap();
        assert_eq!(
            verify_chain(&[e1.clone(), e2.clone(), e3.clone()], &recovery_public).unwrap(),
            &e3
        );
        assert_eq!(e3.open(OFFICE, &office), Err(VaultError::NotForUs));
        let later = k3.seal("devices/x/snapshot.hedy", b"nuevo").unwrap();
        assert!(open(&[k1.clone(), k2.clone()], "devices/x/snapshot.hedy", &later).is_err());

        // The removed PC tries to put itself back: not a member of epoch 3.
        let forged = EpochFile::create(
            &VaultKey::generate(4).unwrap(),
            &[(OFFICE.to_owned(), office_public.clone())],
            OFFICE,
            &office,
        )
        .unwrap();
        assert_eq!(
            e3.verify_next(&forged),
            Err(VaultError::Untrusted(Distrust::Signer))
        );
        // Nor can it pretend another member signed it.
        let mut impostor = forged.clone();
        impostor.signer = HOME.to_owned();
        assert_eq!(
            e3.verify_next(&impostor),
            Err(VaultError::Untrusted(Distrust::Signature))
        );
    }

    #[test]
    fn untrusted_epoch_files_are_refused() {
        let (_, recovery) = with_language(UiLanguage::English, || new_recovery().unwrap());
        let (home, home_public) = pc();
        let k1 = VaultKey::generate(1).unwrap();
        let members = [
            (RECOVERY_ID.to_owned(), recovery.public()),
            ("home".to_owned(), home_public),
        ];
        let e1 = EpochFile::create(&k1, &members, RECOVERY_ID, &recovery).unwrap();

        // A first epoch signed by a PC, or for another recovery key.
        let by_pc = EpochFile::create(&k1, &members, "home", &home).unwrap();
        assert!(by_pc.verify_first(&recovery.public()).is_err());
        let (_, other) = with_language(UiLanguage::English, || new_recovery().unwrap());
        assert!(e1.verify_first(&other.public()).is_err());

        // A member added after signing; a skipped epoch; an empty chain.
        let mut tampered = e1.clone();
        tampered.members.push(tampered.members[1].clone());
        assert_eq!(
            tampered.verify_first(&recovery.public()),
            Err(VaultError::Untrusted(Distrust::Signature))
        );
        let k3 = VaultKey::generate(3).unwrap();
        let e3 = EpochFile::create(&k3, &members, "home", &home).unwrap();
        assert_eq!(
            e1.verify_next(&e3),
            Err(VaultError::Untrusted(Distrust::Sequence))
        );
        assert!(verify_chain(&[], &recovery.public()).is_err());

        // A wrapped key copied into another epoch's file does not open.
        let mut moved = e3.clone();
        moved.members[1].wrapped_key = e1.members[1].wrapped_key.clone();
        assert_eq!(moved.open("home", &home), Err(VaultError::Corrupt));

        let json = serde_json::to_string(&e1).unwrap();
        assert_eq!(serde_json::from_str::<EpochFile>(&json).unwrap(), e1);
    }

    #[test]
    fn personal_epoch_files_are_written_as_before_teams() {
        let (_, recovery) = with_language(UiLanguage::English, || new_recovery().unwrap());
        let (_, home_public) = pc();
        let e1 = EpochFile::create(
            &VaultKey::generate(1).unwrap(),
            &[
                (RECOVERY_ID.to_owned(), recovery.public()),
                ("00000000000000aa".to_owned(), home_public),
            ],
            RECOVERY_ID,
            &recovery,
        )
        .unwrap();
        // No seat fields: the signed bytes of a file written before teams
        // are the same, so its signature still verifies.
        let json: serde_json::Value = serde_json::to_value(&e1).unwrap();
        for member in json["members"].as_array().unwrap() {
            let keys: Vec<&str> = member
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(keys, ["id", "public", "wrapped_key"]);
        }
        let back: EpochFile = serde_json::from_value(json).unwrap();
        back.verify_first(&recovery.public()).unwrap();
    }

    /// An epoch's members as the tests change them.
    type Seated = Vec<(String, MemberPublic, Option<Seat>)>;

    /// A team: the recovery key, an admin (Ana, one PC) and a member (Luis,
    /// one PC), plus the PCs and keys the tests below use.
    struct Team {
        recovery: MemberSecrets,
        ana: MemberSecrets,
        luis: MemberSecrets,
        e1: EpochFile,
    }

    const ANA_PC: &str = "00000000000000a1";
    const LUIS_PC: &str = "00000000000000b1";
    const LUIS_LAPTOP: &str = "00000000000000b2";
    const MARTA_PC: &str = "00000000000000c1";

    fn person(name: &str) -> PersonId {
        PersonId(
            match name {
                "ana" => "aaaaaaaaaaaaaaaa",
                "luis" => "bbbbbbbbbbbbbbbb",
                _ => "cccccccccccccccc",
            }
            .to_owned(),
        )
    }

    fn seat(name: &str, role: Role) -> Option<Seat> {
        Some(Seat {
            person: person(name),
            role,
        })
    }

    impl Team {
        fn new() -> Self {
            let (_, recovery) = with_language(UiLanguage::English, || new_recovery().unwrap());
            let (ana, ana_public) = pc();
            let (luis, luis_public) = pc();
            let e1 = EpochFile::create_seated(
                &VaultKey::generate(1).unwrap(),
                &[
                    (RECOVERY_ID.to_owned(), recovery.public(), None),
                    (ANA_PC.to_owned(), ana_public, seat("ana", Role::Admin)),
                    (LUIS_PC.to_owned(), luis_public, seat("luis", Role::Member)),
                ],
                RECOVERY_ID,
                &recovery,
            )
            .unwrap();
            e1.verify_first(&recovery.public()).unwrap();
            Self {
                recovery,
                ana,
                luis,
                e1,
            }
        }

        /// Epoch 1's members, changed by `change`, signed by `signer`.
        fn next(&self, signer: &str, change: impl FnOnce(&mut Seated)) -> EpochFile {
            let mut members: Vec<_> = self
                .e1
                .members
                .iter()
                .map(|m| (m.id.clone(), m.public.clone(), m.seat()))
                .collect();
            change(&mut members);
            let keys = match signer {
                ANA_PC => &self.ana,
                LUIS_PC => &self.luis,
                _ => &self.recovery,
            };
            EpochFile::create_seated(&VaultKey::generate(2).unwrap(), &members, signer, keys)
                .unwrap()
        }
    }

    fn set_role(members: &mut Seated, id: &str, role: Role) {
        let member = members.iter_mut().find(|m| m.0 == id).unwrap();
        member.2.as_mut().unwrap().role = role;
    }

    #[test]
    fn a_team_admin_adds_removes_and_changes_roles() {
        let team = Team::new();
        // Marta joins (an invite Ana made), and Luis becomes an admin.
        let grown = team.next(ANA_PC, |m| {
            m.push((MARTA_PC.into(), pc().1, seat("marta", Role::Member)));
            set_role(m, LUIS_PC, Role::Admin);
        });
        team.e1.verify_next(&grown).unwrap();
        // Ana removes Luis.
        let smaller = team.next(ANA_PC, |m| m.retain(|m| m.0 != LUIS_PC));
        team.e1.verify_next(&smaller).unwrap();
        // The recovery key may do anything but leave.
        team.e1
            .verify_next(&team.next(RECOVERY_ID, |m| m.retain(|m| m.0 != ANA_PC)))
            .unwrap();
        assert_eq!(
            team.e1
                .verify_next(&team.next(ANA_PC, |m| m.retain(|m| m.0 != RECOVERY_ID))),
            Err(VaultError::Untrusted(Distrust::Members))
        );
    }

    #[test]
    fn a_team_member_only_adds_or_removes_their_own_pcs() {
        let team = Team::new();
        let refused = Err(VaultError::Untrusted(Distrust::Members));
        let laptop = pc().1;

        // Their own new laptop, as a member: fine. Leaving: fine.
        let own = team.next(LUIS_PC, |m| {
            m.push((
                LUIS_LAPTOP.into(),
                laptop.clone(),
                seat("luis", Role::Member),
            ));
        });
        team.e1.verify_next(&own).unwrap();
        let leaves = team.next(LUIS_PC, |m| m.retain(|m| m.0 != LUIS_PC));
        team.e1.verify_next(&leaves).unwrap();

        // Their laptop as an admin, or a PC for someone else, or a newcomer.
        for (id, seat) in [
            (LUIS_LAPTOP, seat("luis", Role::Admin)),
            (LUIS_LAPTOP, seat("ana", Role::Admin)),
            (MARTA_PC, seat("marta", Role::Member)),
        ] {
            let added = team.next(LUIS_PC, |m| m.push((id.into(), laptop.clone(), seat)));
            assert_eq!(team.e1.verify_next(&added), refused, "{id}");
        }
        // Promoting themselves, demoting or removing the admin.
        assert_eq!(
            team.e1
                .verify_next(&team.next(LUIS_PC, |m| set_role(m, LUIS_PC, Role::Admin))),
            refused
        );
        assert_eq!(
            team.e1
                .verify_next(&team.next(LUIS_PC, |m| set_role(m, ANA_PC, Role::Member))),
            refused
        );
        assert_eq!(
            team.e1
                .verify_next(&team.next(LUIS_PC, |m| m.retain(|m| m.0 != ANA_PC))),
            refused
        );
        // Moving a PC to another person, or changing its keys.
        assert_eq!(
            team.e1.verify_next(&team.next(LUIS_PC, |m| {
                m.iter_mut().find(|m| m.0 == LUIS_PC).unwrap().2 = seat("ana", Role::Admin);
            })),
            refused
        );
        assert_eq!(
            team.e1.verify_next(&team.next(ANA_PC, |m| {
                m.iter_mut().find(|m| m.0 == LUIS_PC).unwrap().1 = laptop.clone();
            })),
            refused
        );
    }

    #[test]
    fn team_seats_must_hold_together() {
        let team = Team::new();
        let refused = Err(VaultError::Untrusted(Distrust::Members));
        let check = |change: &dyn Fn(&mut Seated)| {
            team.e1.verify_next(&team.next(RECOVERY_ID, |m| change(m)))
        };
        // A PC with no seat in a team; the recovery key with one.
        assert_eq!(check(&|m| m.push((MARTA_PC.into(), pc().1, None))), refused);
        assert_eq!(check(&|m| m[0].2 = seat("marta", Role::Admin)), refused);
        // One person, two roles; a person id that is not one.
        assert_eq!(
            check(&|m| m.push((LUIS_LAPTOP.into(), pc().1, seat("luis", Role::Admin)))),
            refused
        );
        assert_eq!(
            check(&|m| {
                m.push((
                    MARTA_PC.into(),
                    pc().1,
                    Some(Seat {
                        person: PersonId("../marta".into()),
                        role: Role::Member,
                    }),
                ))
            }),
            refused
        );
        // A person without a role (a file written by hand).
        let mut half = team.e1.clone();
        half.members[2].role = None;
        assert_eq!(half.valid_members(), refused);
        // A personal vault never turns into a team without its words.
        let personal = EpochFile::create(
            &VaultKey::generate(1).unwrap(),
            &[
                (RECOVERY_ID.to_owned(), team.recovery.public()),
                (ANA_PC.to_owned(), team.ana.public()),
            ],
            RECOVERY_ID,
            &team.recovery,
        )
        .unwrap();
        let seated = EpochFile::create_seated(
            &VaultKey::generate(2).unwrap(),
            &[
                (RECOVERY_ID.to_owned(), team.recovery.public(), None),
                (
                    ANA_PC.to_owned(),
                    team.ana.public(),
                    seat("ana", Role::Admin),
                ),
            ],
            ANA_PC,
            &team.ana,
        )
        .unwrap();
        assert_eq!(personal.verify_next(&seated), refused);
    }
}
