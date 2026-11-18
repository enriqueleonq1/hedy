//! One-time team invites (docs/decisions.md §35): a short code, typed by the
//! person who joins, that both PCs check through the team repo without the
//! code (or anything that tests a guess) ever being written there.
//!
//! - **The code** is a number naming the invite plus four words from the
//!   BIP39 list (44 bits): `7-tigre-lima-nube-mar`. Words are kept without
//!   accents (but with their `ñ`), and either the Spanish or the English
//!   list is read back.
//! - **The check** is SPAKE2 (RustCrypto's `spake2`, as Magic Wormhole uses
//!   it): the inviting PC writes its message in the invite, the joiner
//!   answers with its own and seals its request under the key they now
//!   share. Someone with only the repo cannot test codes offline: each guess
//!   needs a request, and the inviting PC burns the invite on the first one
//!   that does not open. Both sides bind a context (the team repo) into
//!   SPAKE2, so an invite answers for one repo only.
//! - **The shared key** is an [`InviteKey`], not a vault key: what it seals
//!   never opens as a vault file, since the joiner is not trusted yet.
//! - **Restarts**: the inviting PC keeps a 32-byte seed in the keyring, and
//!   its SPAKE2 side is rebuilt from it, so the invite outlives the app.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use bip39::Language;
use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::SeedableRng;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use zeroize::Zeroizing;

use crate::adapters::vault::{self, INVITE_EPOCH, VaultError, VaultKey};
use crate::domain::sync::fold_word;

/// Words in a code, after its number.
const WORDS: usize = 4;
const INVITER: &str = "hedy-team-invite-v1:inviter:";
const JOINER: &str = "hedy-team-invite-v1:joiner:";
/// How an invite key starts in text, so it is never read as a vault key.
const KEY_PREFIX: &str = "invite:";

type Result<T> = std::result::Result<T, VaultError>;

/// The key an invite's two PCs share: it seals the joiner's request and
/// the answer, and nothing else opens with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteKey(VaultKey);

impl InviteKey {
    pub fn seal(&self, path: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        self.0.seal(path, plaintext)
    }

    pub fn open(&self, path: &str, sealed: &[u8]) -> Result<Vec<u8>> {
        vault::open_with(&self.0, path, sealed)
    }

    /// For the keyring: `invite:<base64>`. [`Self::decode`] reads it back.
    pub fn encode(&self) -> String {
        format!("{KEY_PREFIX}{}", self.0.key_base64())
    }

    pub fn decode(text: &str) -> Result<Self> {
        let key = text.strip_prefix(KEY_PREFIX).ok_or(VaultError::Corrupt)?;
        Ok(Self(VaultKey::from_base64(INVITE_EPOCH, key)?))
    }
}

/// A new code for invite number `nameplate`, with words from the
/// interface's language.
pub fn new_code(nameplate: u32) -> Result<String> {
    let list = match crate::i18n::language() {
        crate::i18n::Language::Spanish => Language::Spanish,
        _ => Language::English,
    }
    .word_list();
    let mut code = nameplate.to_string();
    for _ in 0..WORDS {
        let mut bytes = [0u8; 2];
        getrandom::fill(&mut bytes).map_err(|_| VaultError::Random)?;
        // 2048 words: the low 11 bits are uniform.
        let index = usize::from(u16::from_le_bytes(bytes)) % list.len();
        code.push('-');
        code.push_str(&fold_word(list[index]));
    }
    Ok(code)
}

/// A code as typed (any case, accents or not, words apart by spaces,
/// dashes or any other punctuation): (the invite's number, the code in the
/// form both PCs use). `None` when it is not one, so a typo is caught here
/// instead of burning the invite.
pub fn parse_code(typed: &str) -> Option<(u32, String)> {
    // Accents may come as separate marks (`a` + `´`): part of the word.
    let mut parts = typed
        .split(|c: char| !c.is_alphanumeric() && !('\u{300}'..='\u{36f}').contains(&c))
        .filter(|p| !p.is_empty());
    let nameplate: u32 = parts.next()?.parse().ok()?;
    let words: Vec<String> = parts.map(fold_word).collect();
    let known = |word: &String| {
        [Language::Spanish, Language::English]
            .iter()
            .any(|l| l.word_list().iter().any(|w| fold_word(w) == *word))
    };
    (words.len() == WORDS && words.iter().all(known))
        .then(|| (nameplate, format!("{nameplate}-{}", words.join("-"))))
}

fn identities(context: &str) -> (Identity, Identity) {
    (
        Identity::new(format!("{INVITER}{context}").as_bytes()),
        Identity::new(format!("{JOINER}{context}").as_bytes()),
    )
}

fn inviter_side(seed: &[u8; 32], code: &str, context: &str) -> (Spake2<Ed25519Group>, Vec<u8>) {
    let (inviter, joiner) = identities(context);
    Spake2::<Ed25519Group>::start_a_with_rng(
        &Password::new(code.as_bytes()),
        &inviter,
        &joiner,
        ChaCha20Rng::from_seed(*seed),
    )
}

fn shared_key(spake_key: &[u8]) -> InviteKey {
    InviteKey(VaultKey::derive(
        INVITE_EPOCH,
        b"hedy-team-invite-v1",
        spake_key,
    ))
}

fn decode_seed(secret: &str) -> Result<Zeroizing<[u8; 32]>> {
    let bytes = Zeroizing::new(
        BASE64
            .decode(secret.trim())
            .map_err(|_| VaultError::Corrupt)?,
    );
    let mut seed = Zeroizing::new([0u8; 32]);
    if bytes.len() != seed.len() {
        return Err(VaultError::Corrupt);
    }
    seed.copy_from_slice(&bytes);
    Ok(seed)
}

/// The inviting PC's side of a new invite for `context` (the team repo):
/// (its secret, for the keyring until the invite is used or expires; its
/// message, for the invite file).
pub fn start(code: &str, context: &str) -> Result<(String, String)> {
    let mut seed = Zeroizing::new([0u8; 32]);
    getrandom::fill(seed.as_mut()).map_err(|_| VaultError::Random)?;
    let (_, message) = inviter_side(&seed, code, context);
    Ok((BASE64.encode(seed.as_ref()), BASE64.encode(message)))
}

/// The inviting PC reads the joiner's message: the key they now share.
/// SPAKE2 does not tell a wrong code apart: with one, the key differs, and
/// the joiner's request (sealed with theirs) does not open with it. Each
/// call is one guess: the caller burns the invite before making it.
pub fn accept(secret: &str, code: &str, context: &str, joiner_message: &str) -> Result<InviteKey> {
    let seed = decode_seed(secret)?;
    let message = BASE64
        .decode(joiner_message.trim())
        .map_err(|_| VaultError::Corrupt)?;
    let (side, _) = inviter_side(&seed, code, context);
    let key = Zeroizing::new(side.finish(&message).map_err(|_| VaultError::Corrupt)?);
    Ok(shared_key(&key))
}

/// The joiner's side: (its message, for its request; the key it shares
/// with the inviting PC if the code and the context were right).
pub fn join(code: &str, context: &str, inviter_message: &str) -> Result<(String, InviteKey)> {
    let theirs = BASE64
        .decode(inviter_message.trim())
        .map_err(|_| VaultError::Corrupt)?;
    let (inviter, joiner) = identities(context);
    let (side, message) =
        Spake2::<Ed25519Group>::start_b(&Password::new(code.as_bytes()), &inviter, &joiner);
    let key = Zeroizing::new(side.finish(&theirs).map_err(|_| VaultError::Corrupt)?);
    Ok((BASE64.encode(message), shared_key(&key)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language as UiLanguage, with_language};

    const REPO: &str = "empresa-org/hedy-team";

    #[test]
    fn codes_read_back_however_they_are_typed() {
        let code = with_language(UiLanguage::Spanish, || new_code(7).unwrap());
        assert_eq!(code.split('-').count(), 1 + WORDS);
        assert_eq!(parse_code(&code), Some((7, code.clone())));
        let typed = format!("  {}  ", code.to_uppercase().replace('-', " "));
        assert_eq!(parse_code(&typed), Some((7, code.clone())));
        // Plain letters to type: no accents (an `ñ` stays).
        for _ in 0..200 {
            let code = with_language(UiLanguage::Spanish, || new_code(1).unwrap());
            assert!(code.chars().all(|c| c.is_ascii() || c == 'ñ'), "{code}");
        }
        // With the accents the list has, as one character or two, other
        // separators, or from the English list.
        for typed in [
            "12-ábaco-ÁRBOL-nube-mar",
            "12 a\u{301}baco, árbol – nube_mar.",
            "012-abaco-arbol-nube-mar",
        ] {
            assert_eq!(
                parse_code(typed),
                Some((12, "12-abaco-arbol-nube-mar".into())),
                "{typed}"
            );
        }
        assert_eq!(
            parse_code("5 AÑO baño araña mar"),
            Some((5, "5-año-baño-araña-mar".into()))
        );
        let english = with_language(UiLanguage::English, || new_code(3).unwrap());
        assert_eq!(parse_code(&english), Some((3, english.clone())));

        // A typo, a missing word or number, one too many, a number too big.
        for typed in [
            "7-tigre-lima-nube-marx",
            "5-ano-baño-araña-mar",
            "7-tigre-lima-nube",
            "7-tigre-lima-nube-mar-sol",
            "tigre-lima-nube-mar-sol",
            "99999999999-tigre-lima-nube-mar",
            "",
        ] {
            assert_eq!(parse_code(typed), None, "{typed}");
        }
    }

    #[test]
    fn both_pcs_share_a_key_only_with_the_same_code_and_repo() {
        let code = "7-tigre-lima-nube-mar";
        let (secret, invite) = start(code, REPO).unwrap();
        // The inviting PC can rebuild its side after a restart.
        let (_, again) = inviter_side(&decode_seed(&secret).unwrap(), code, REPO);
        assert_eq!(BASE64.encode(again), invite);
        assert!(!invite.contains("tigre") && !secret.contains("tigre"));

        let (request, joiner_key) = join(code, REPO, &invite).unwrap();
        let inviter_key = accept(&secret, code, REPO, &request).unwrap();
        assert_eq!(inviter_key, joiner_key);
        let path = "invites/7/request.hedy";
        let sealed = joiner_key.seal(path, b"mis claves").unwrap();
        assert_eq!(inviter_key.open(path, &sealed).unwrap(), b"mis claves");
        assert_eq!(
            InviteKey::decode(&inviter_key.encode()).unwrap(),
            inviter_key
        );

        // A wrong code, or the invite answered for another repo, gives
        // another key: the request does not open, and nothing says why.
        for (code_typed, repo) in [("7-tigre-lima-nube-sol", REPO), (code, "otro/repo")] {
            let (request, wrong_key) = join(code_typed, repo, &invite).unwrap();
            let inviter_key = accept(&secret, code, REPO, &request).unwrap();
            assert_ne!(inviter_key, wrong_key);
            let sealed = wrong_key.seal(path, b"adivino").unwrap();
            assert_eq!(inviter_key.open(path, &sealed), Err(VaultError::Corrupt));
        }

        // Junk instead of a message, or another invite's secret.
        assert!(join(code, REPO, "nada").is_err());
        assert!(accept(&secret, code, REPO, &invite).is_err());
        assert!(accept("corto", code, REPO, &request).is_err());
    }

    #[test]
    fn an_invite_key_never_opens_or_reads_as_a_vault_key() {
        let (_, invite) = start("1-sol-mar-luz-pan", REPO).unwrap();
        let (_, key) = join("1-sol-mar-luz-pan", REPO, &invite).unwrap();
        // What the joiner seals, even at a vault path, is not a vault file.
        let path = "devices/abababababababab/changes/0000000001.hedy";
        let sealed = key.seal(path, b"cambio").unwrap();
        assert_eq!(
            vault::open(std::slice::from_ref(&key.0), path, &sealed),
            Err(VaultError::NotForUs)
        );
        // Its text is not a vault key's, nor is epoch 0.
        assert!(VaultKey::decode(&key.encode()).is_err());
        assert!(VaultKey::decode(&format!("0:{}", key.0.key_base64())).is_err());
        // And a vault key is not an invite key.
        let epoch_key = VaultKey::generate(1).unwrap();
        assert!(InviteKey::decode(&epoch_key.encode()).is_err());
        assert_eq!(
            key.open("x", &epoch_key.seal("x", b"vault").unwrap()),
            Err(VaultError::NotForUs)
        );
    }
}
