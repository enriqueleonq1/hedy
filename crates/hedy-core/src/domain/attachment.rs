//! Images attached to a message: what the user may attach (type, size,
//! count), how they are kept with the history, and what goes to the agent.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::DomainError;
use crate::tr;

/// Largest image the composer accepts, in bytes.
pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
/// Images per message.
pub const MAX_IMAGES: usize = 4;
/// Characters kept of a file name shown next to an image.
const MAX_NAME_CHARS: usize = 80;

/// The image formats every supported CLI documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageType {
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/gif")]
    Gif,
    #[serde(rename = "image/webp")]
    Webp,
}

impl ImageType {
    /// From the file's first bytes: the name or what the browser says are
    /// never trusted.
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(Self::Gif)
        } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            Some(Self::Webp)
        } else {
            None
        }
    }

    pub fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Gif => "gif",
            Self::Webp => "webp",
        }
    }
}

/// An image kept with a message of the history. The file lives in Hedy's
/// data folder (`attachments/<task id>/<file>`), never in the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    /// The name the user's file had, for people (alt text).
    pub name: String,
    /// The stored file's name inside the task's attachments folder.
    pub file: String,
    pub media_type: ImageType,
}

/// An image that goes to the agent with a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptImage {
    /// The stored file (absolute).
    pub path: PathBuf,
    pub media_type: ImageType,
}

/// Whether an agent takes images with a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageSupport {
    Yes,
    /// It depends on what the agent says when it starts (ACP's
    /// `promptCapabilities.image`); without it, only the text goes.
    IfAgentAllows,
    No,
}

impl ImageSupport {
    pub fn accepts(self) -> bool {
        self != Self::No
    }
}

/// Checks one image the user attached. Returns its real type.
pub fn check_image(name: &str, bytes: &[u8]) -> Result<ImageType, DomainError> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(DomainError::ImageTooLarge(display_name(name)));
    }
    ImageType::sniff(bytes).ok_or_else(|| DomainError::UnsupportedImage(display_name(name)))
}

/// Checks how many images go in one message.
pub fn check_count(count: usize) -> Result<(), DomainError> {
    if count > MAX_IMAGES {
        Err(DomainError::TooManyImages(MAX_IMAGES))
    } else {
        Ok(())
    }
}

/// The file name to show: no folders, no control characters, not too long.
pub fn display_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let clean: String = base
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME_CHARS)
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        tr!("imagen", "image")
    } else {
        clean.to_owned()
    }
}

/// Whether `file` can be the name of a stored attachment: Hedy names them
/// itself (letters, digits, `-` and one `.`), so anything else, such as a
/// path, is refused before touching the disk.
pub fn is_stored_name(file: &str) -> bool {
    !file.is_empty()
        && file.len() <= 64
        && !file.starts_with('.')
        && file.matches('.').count() == 1
        && file
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    #[test]
    fn the_type_comes_from_the_bytes() {
        assert_eq!(ImageType::sniff(PNG), Some(ImageType::Png));
        assert_eq!(
            ImageType::sniff(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(ImageType::Jpeg)
        );
        assert_eq!(ImageType::sniff(b"GIF89a.."), Some(ImageType::Gif));
        assert_eq!(
            ImageType::sniff(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(ImageType::Webp)
        );
        assert_eq!(ImageType::sniff(b"<svg xmlns="), None);
        assert_eq!(ImageType::sniff(b""), None);
    }

    #[test]
    fn media_types_serialize_as_mime() {
        let json = serde_json::to_string(&ImageType::Jpeg).unwrap();
        assert_eq!(json, "\"image/jpeg\"");
        assert_eq!(ImageType::Webp.extension(), "webp");
    }

    #[test]
    fn size_type_and_count_are_checked() {
        assert_eq!(check_image("a.png", PNG), Ok(ImageType::Png));
        let big = [PNG, &vec![0; MAX_IMAGE_BYTES]].concat();
        assert_eq!(
            check_image("/tmp/big.png", &big),
            Err(DomainError::ImageTooLarge("big.png".into()))
        );
        assert_eq!(
            check_image("x.svg", b"<svg/>"),
            Err(DomainError::UnsupportedImage("x.svg".into()))
        );
        assert!(check_count(MAX_IMAGES).is_ok());
        assert_eq!(
            check_count(MAX_IMAGES + 1),
            Err(DomainError::TooManyImages(MAX_IMAGES))
        );
    }

    #[test]
    fn names_are_cleaned_and_stored_names_are_strict() {
        assert_eq!(display_name("C:\\shots\\pantalla\n.png"), "pantalla.png");
        assert_eq!(display_name("   "), "imagen");
        assert!(is_stored_name("18c2f-0.png"));
        for bad in ["", "../x.png", "a/b.png", ".png", "a.b.png", "a png"] {
            assert!(!is_stored_name(bad), "{bad}");
        }
    }
}
