//! Images in each agent's official prompt format (docs/decisions.md §24).
//! The files were written by Hedy in its data folder; they are read here
//! only to send them.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};

use crate::application::ports::{PortError, PortResult};
use crate::domain::PromptImage;
use crate::tr;

/// The stored image, base64-encoded.
pub(crate) fn base64_of(image: &PromptImage) -> PortResult<String> {
    std::fs::read(&image.path)
        .map(|bytes| BASE64.encode(bytes))
        .map_err(|e| {
            PortError::new(tr!(
                "no se pudo leer la imagen adjunta: {e}",
                "could not read the attached image: {e}"
            ))
        })
}

/// Claude Code (`--input-format stream-json`): the Messages API content
/// blocks, a text block then one base64 image block per image.
pub(crate) fn claude_content(text: &str, images: &[PromptImage]) -> PortResult<Value> {
    if images.is_empty() {
        return Ok(Value::String(text.to_owned()));
    }
    let mut blocks = vec![json!({"type": "text", "text": text})];
    for image in images {
        blocks.push(json!({"type": "image", "source": {
            "type": "base64",
            "media_type": image.media_type.media_type(),
            "data": base64_of(image)?,
        }}));
    }
    Ok(Value::Array(blocks))
}

/// Codex app-server `turn/start` input: the text, then a `localImage` item
/// per image (the app-server reads the file itself).
pub(crate) fn codex_input(text: &str, images: &[PromptImage]) -> Value {
    let mut items = vec![json!({"type": "text", "text": text})];
    items.extend(
        images
            .iter()
            .map(|image| json!({"type": "localImage", "path": image.path})),
    );
    Value::Array(items)
}

/// ACP `session/prompt`: a text block, then an `image` block per image
/// (only for agents whose `promptCapabilities.image` is true).
pub(crate) fn acp_prompt(text: &str, images: &[PromptImage]) -> PortResult<Value> {
    let mut blocks = vec![json!({"type": "text", "text": text})];
    for image in images {
        blocks.push(json!({
            "type": "image",
            "mimeType": image.media_type.media_type(),
            "data": base64_of(image)?,
        }));
    }
    Ok(Value::Array(blocks))
}

/// OpenAI-compatible chat completions (Kimi, Gemini): a user message whose
/// content is a text part and an `image_url` part per image, as a base64
/// data URL (public URLs are not accepted by Kimi).
pub(crate) fn openai_content(text: &str, images: &[PromptImage]) -> PortResult<Value> {
    if images.is_empty() {
        return Ok(Value::String(text.to_owned()));
    }
    let mut parts = vec![json!({"type": "text", "text": text})];
    for image in images {
        let url = format!(
            "data:{};base64,{}",
            image.media_type.media_type(),
            base64_of(image)?
        );
        parts.push(json!({"type": "image_url", "image_url": {"url": url}}));
    }
    Ok(Value::Array(parts))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::domain::ImageType;

    /// A PNG file in `dir`, as Hedy stores attachments.
    pub(crate) fn png(dir: &std::path::Path) -> PromptImage {
        let path = dir.join("a-0.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\nxx").unwrap();
        PromptImage {
            path,
            media_type: ImageType::Png,
        }
    }

    const PNG_BASE64: &str = "iVBORw0KGgp4eA==";

    #[test]
    fn each_agent_gets_its_own_format() {
        let tmp = tempfile::tempdir().unwrap();
        let image = png(tmp.path());
        let images = [image.clone()];

        assert_eq!(claude_content("hola", &[]).unwrap(), json!("hola"));
        assert_eq!(
            claude_content("mira", &images).unwrap(),
            json!([{"type": "text", "text": "mira"},
                   {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": PNG_BASE64}}])
        );
        assert_eq!(
            codex_input("mira", &images),
            json!([{"type": "text", "text": "mira"}, {"type": "localImage", "path": image.path}])
        );
        assert_eq!(
            acp_prompt("mira", &images).unwrap(),
            json!([{"type": "text", "text": "mira"},
                   {"type": "image", "mimeType": "image/png", "data": PNG_BASE64}])
        );
        assert_eq!(
            openai_content("mira", &images).unwrap(),
            json!([{"type": "text", "text": "mira"},
                   {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{PNG_BASE64}")}}])
        );
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_silent_drop() {
        let image = PromptImage {
            path: "/nonexistent/hedy/a.png".into(),
            media_type: ImageType::Png,
        };
        assert!(claude_content("x", &[image]).is_err());
    }
}
