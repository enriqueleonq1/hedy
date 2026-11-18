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
//!   that does not open.
//! - **Restarts**: the inviting PC keeps a 32-byte seed in the keyring, and
//!   its SPAKE2 side is rebuilt from it, so the invite outlives the app.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use bip39::Language;
use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::SeedableRng;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use zeroize::Zeroizing;

use crate::adapters::vault::{VaultError, VaultKey};
use crate::domain::sync::fold_word;

/// Words in a code, after its number.
const WORDS: usize = 4;
const INVITER: &[u8] = b"hedy-team-invite-v1:inviter";
const JOINER: &[u8] = b"hedy-team-invite-v1:joiner";
/// The shared key seals like an epoch key, under an epoch no vault uses.
const INVITE_EPOCH: u32 = 0;

type Result<T> = std::result::Result<T, VaultError>;

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
        code.push_str(&fold(list[index]));
    }
    Ok(code)
}

/// A code as typed (any case, spaces or dashes, accents or not), in the
/// form both PCs use; `None` when it is not one, so a typo is caught here
/// instead of burning the invite.
pub fn parse_code(typed: &str) -> Option<String> {
    let mut parts = typed
        .split(|c: char| c == '-' || c.is_whitespace())
        .filter(|p| !p.is_empty());
    let nameplate: u32 = parts.next()?.parse().ok()?;
    let words: Vec<String> = parts.map(fold).collect();
    let known = |word: &String| {
        [Language::Spanish, Language::English]
            .iter()
            .any(|l| l.word_list().iter().any(|w| fold(w) == *word))
    };
    (words.len() == WORDS && words.iter().all(known))
        .then(|| format!("{nameplate}-{}", words.join("-")))
}

/// A word without its accents, however they are written (the BIP39
/// Spanish list keeps them as separate combining marks: `a` + `´`), but
/// with its `ñ`.
fn fold(word: &str) -> String {
    let mut out = String::new();
    for c in fold_word(word).chars() {
        if c == '\u{303}' && out.ends_with('n') {
            out.pop();
            out.push('ñ');
        } else if !('\u{300}'..='\u{36f}').contains(&c) {
            out.push(c);
        }
    }
    out
}

/// The number naming the invite a (parsed) code is for.
pub fn nameplate(code: &str) -> Option<u32> {
    code.split('-').next()?.parse().ok()
}

fn inviter_side(seed: &[u8; 32], code: &str) -> (Spake2<Ed25519Group>, Vec<u8>) {
    Spake2::<Ed25519Group>::start_a_with_rng(
        &Password::new(code.as_bytes()),
        &Identity::new(INVITER),
        &Identity::new(JOINER),
        ChaCha20Rng::from_seed(*seed),
    )
}

fn shared_key(spake_key: &[u8]) -> VaultKey {
    VaultKey::derive(INVITE_EPOCH, b"hedy-team-invite-v1", spake_key)
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

/// The inviting PC's side of a new invite: (its secret, for the keyring
/// until the invite is used or expires; its message, for the invite file).
pub fn start(code: &str) -> Result<(String, String)> {
    let mut seed = Zeroizing::new([0u8; 32]);
    getrandom::fill(seed.as_mut()).map_err(|_| VaultError::Random)?;
    let (_, message) = inviter_side(&seed, code);
    Ok((BASE64.encode(seed.as_ref()), BASE64.encode(message)))
}

/// The inviting PC reads the joiner's message: the key they now share.
/// SPAKE2 does not tell a wrong code apart: with one, the key differs, and
/// the joiner's request (sealed with theirs) does not open with it.
pub fn accept(secret: &str, code: &str, joiner_message: &str) -> Result<VaultKey> {
    let seed = decode_seed(secret)?;
    let message = BASE64
        .decode(joiner_message.trim())
        .map_err(|_| VaultError::Corrupt)?;
    let (side, _) = inviter_side(&seed, code);
    let key = Zeroizing::new(side.finish(&message).map_err(|_| VaultError::Corrupt)?);
    Ok(shared_key(&key))
}

/// The joiner's side: (its message, for its request; the key it shares
/// with the inviting PC if the code was right).
pub fn join(code: &str, inviter_message: &str) -> Result<(String, VaultKey)> {
    let theirs = BASE64
        .decode(inviter_message.trim())
        .map_err(|_| VaultError::Corrupt)?;
    let (side, message) = Spake2::<Ed25519Group>::start_b(
        &Password::new(code.as_bytes()),
        &Identity::new(INVITER),
        &Identity::new(JOINER),
    );
    let key = Zeroizing::new(side.finish(&theirs).map_err(|_| VaultError::Corrupt)?);
    Ok((BASE64.encode(message), shared_key(&key)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::vault;
    use crate::i18n::{Language as UiLanguage, with_language};

    #[test]
    fn codes_read_back_however_they_are_typed() {
        let code = with_language(UiLanguage::Spanish, || new_code(7).unwrap());
        assert_eq!(nameplate(&code), Some(7));
        assert_eq!(code.split('-').count(), 1 + WORDS);
        // Plain letters to type: no accents (an `ñ` stays).
        for _ in 0..200 {
            let code = with_language(UiLanguage::Spanish, || new_code(1).unwrap());
            assert!(code.chars().all(|c| c.is_ascii() || c == 'ñ'), "{code}");
        }
        assert_eq!(parse_code(&code).as_deref(), Some(code.as_str()));
        assert_eq!(
            parse_code(&format!("  {}  ", code.to_uppercase().replace('-', " "))).as_deref(),
            Some(code.as_str())
        );
        // With the accents the list has, or from the English list.
        assert_eq!(
            parse_code("12-ábaco-ÁRBOL-nube-mar").as_deref(),
            Some("12-abaco-arbol-nube-mar")
        );
        assert_eq!(
            parse_code("5 AÑO baño araña mar").as_deref(),
            Some("5-año-baño-araña-mar")
        );
        assert_eq!(parse_code("5-ano-baño-araña-mar"), None);
        let english = with_language(UiLanguage::English, || new_code(3).unwrap());
        assert_eq!(parse_code(&english).as_deref(), Some(english.as_str()));

        // A typo, a missing word or number: caught before any try.
        assert_eq!(parse_code("7-tigre-lima-nube-farox"), None);
        assert_eq!(parse_code("7-tigre-lima-nube"), None);
        assert_eq!(parse_code("tigre-lima-nube-faro-mar"), None);
        assert_eq!(parse_code(""), None);
    }

    #[test]
    fn both_pcs_share_a_key_only_with_the_same_code() {
        let code = "7-tigre-lima-nube-faro";
        let (secret, invite) = start(code).unwrap();
        // The inviting PC can rebuild its side after a restart.
        let (_, again) = inviter_side(&decode_seed(&secret).unwrap(), code);
        assert_eq!(BASE64.encode(again), invite);
        assert!(!invite.contains("tigre") && !secret.contains("tigre"));

        let (request, joiner_key) = join(code, &invite).unwrap();
        let inviter_key = accept(&secret, code, &request).unwrap();
        assert_eq!(inviter_key, joiner_key);
        let path = "invites/7/request.hedy";
        let sealed = joiner_key.seal(path, b"mis claves").unwrap();
        assert_eq!(
            vault::open(&[inviter_key], path, &sealed).unwrap(),
            b"mis claves"
        );

        // A wrong code gives another key: the request does not open, and
        // nothing says which word was wrong.
        let (request, wrong_key) = join("7-tigre-lima-nube-mar", &invite).unwrap();
        let inviter_key = accept(&secret, code, &request).unwrap();
        assert_ne!(inviter_key, wrong_key);
        let sealed = wrong_key.seal(path, b"adivino").unwrap();
        assert_eq!(
            vault::open(&[inviter_key], path, &sealed),
            Err(VaultError::Corrupt)
        );

        // Junk instead of a message, or another invite's secret.
        assert!(join(code, "nada").is_err());
        assert!(accept(&secret, code, &invite).is_err());
        assert!(accept("corto", code, &request).is_err());
    }
}
