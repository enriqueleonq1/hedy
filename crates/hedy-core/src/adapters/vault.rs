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
    let fold = |w: &str| -> String {
        w.chars()
            .map(|c| match c {
                'á' => 'a',
                'é' => 'e',
                'í' => 'i',
                'ó' => 'o',
                'ú' | 'ü' => 'u',
                other => other,
            })
            .collect()
    };
    words
        .iter()
        .map(|w| {
            language
                .word_list()
                .iter()
                .find(|listed| fold(listed) == fold(w))
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
    pub public: MemberPublic,
    pub wrapped_key: String,
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
    /// `key` sealed for each of `members` (id, public keys), signed by
    /// `signer`.
    pub fn create(
        key: &VaultKey,
        members: &[(String, MemberPublic)],
        signer_id: &str,
        signer: &MemberSecrets,
    ) -> Result<Self> {
        let payload = wrap_payload(key);
        let members = members
            .iter()
            .map(|(id, public)| {
                let sealed = public
                    .box_key()?
                    .seal(&mut crypto_box::aead::OsRng, &payload)
                    .map_err(|_| VaultError::Random)?;
                Ok(Member {
                    id: id.clone(),
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
        self.signed_by(recovery)
    }

    /// Whether `next` may follow `self`: the next epoch, signed by a member
    /// of this one.
    pub fn verify_next(&self, next: &EpochFile) -> Result<()> {
        if next.format != FORMAT || next.epoch != self.epoch + 1 {
            return Err(VaultError::Untrusted(Distrust::Sequence));
        }
        let signer = self
            .member(&next.signer)
            .ok_or(VaultError::Untrusted(Distrust::Signer))?;
        next.signed_by(&signer.public)
    }
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
        members: &[(String, String)],
        signer_id: &str,
        signer: &str,
    ) -> PortResult<(String, String)> {
        let key = VaultKey::generate(epoch).map_err(port)?;
        let members = members
            .iter()
            .map(|(id, public)| Ok((id.clone(), member_public(public)?)))
            .collect::<Result<Vec<_>>>()
            .map_err(port)?;
        let signer = MemberSecrets::decode(signer).map_err(port)?;
        let file = EpochFile::create(&key, &members, signer_id, &signer).map_err(port)?;
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

    fn epoch_members(&self, file: &str) -> PortResult<(u32, Vec<(String, String)>)> {
        let file = epoch_file(file).map_err(port)?;
        let members = file
            .members
            .iter()
            .map(|m| {
                let public = serde_json::to_string(&m.public).expect("public keys serialize");
                (m.id.clone(), public)
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
    fn epochs_let_members_in_and_removed_pcs_out() {
        let (_, recovery) = with_language(UiLanguage::English, || new_recovery().unwrap());
        let recovery_public = recovery.public();
        let (home, home_public) = pc();
        let (office, office_public) = pc();

        // Setup on "home": signed by the recovery key.
        let k1 = VaultKey::generate(1).unwrap();
        let e1 = EpochFile::create(
            &k1,
            &[
                (RECOVERY_ID.into(), recovery_public.clone()),
                ("home".into(), home_public.clone()),
            ],
            RECOVERY_ID,
            &recovery,
        )
        .unwrap();
        e1.verify_first(&recovery_public).unwrap();
        assert_eq!(e1.open("home", &home).unwrap(), k1);
        assert_eq!(e1.open(RECOVERY_ID, &recovery).unwrap(), k1);
        assert_eq!(e1.open("office", &office), Err(VaultError::NotForUs));
        assert_eq!(e1.open("home", &office), Err(VaultError::NotForUs));

        // "office" joins with the words: the recovery key signs epoch 2.
        let k2 = VaultKey::generate(2).unwrap();
        let e2 = EpochFile::create(
            &k2,
            &[
                (RECOVERY_ID.into(), recovery_public.clone()),
                ("home".into(), home_public.clone()),
                ("office".into(), office_public.clone()),
            ],
            RECOVERY_ID,
            &recovery,
        )
        .unwrap();
        e1.verify_next(&e2).unwrap();
        assert_eq!(e2.open("office", &office).unwrap(), k2);

        // "home" removes "office": epoch 3, signed by home.
        let k3 = VaultKey::generate(3).unwrap();
        let e3 = EpochFile::create(
            &k3,
            &[
                (RECOVERY_ID.into(), recovery_public.clone()),
                ("home".into(), home_public.clone()),
            ],
            "home",
            &home,
        )
        .unwrap();
        e2.verify_next(&e3).unwrap();
        assert_eq!(
            verify_chain(&[e1.clone(), e2.clone(), e3.clone()], &recovery_public).unwrap(),
            &e3
        );
        assert_eq!(e3.open("office", &office), Err(VaultError::NotForUs));
        let later = k3.seal("devices/x/snapshot.hedy", b"nuevo").unwrap();
        assert!(open(&[k1.clone(), k2.clone()], "devices/x/snapshot.hedy", &later).is_err());

        // The removed PC tries to put itself back: not a member of epoch 3.
        let forged = EpochFile::create(
            &VaultKey::generate(4).unwrap(),
            &[("office".into(), office_public.clone())],
            "office",
            &office,
        )
        .unwrap();
        assert_eq!(
            e3.verify_next(&forged),
            Err(VaultError::Untrusted(Distrust::Signer))
        );
        // Nor can it pretend another member signed it.
        let mut impostor = forged.clone();
        impostor.signer = "home".into();
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
}
