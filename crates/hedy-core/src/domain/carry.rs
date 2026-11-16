//! Carrying a quick question ("conversation") into a project: the user's
//! questions and the agent's final answers become a clearly delimited context
//! section that opens the new task's prompt, before the user's instruction.
//! Everything here is deterministic and free; only "Resumir con IA" asks an
//! agent (see `summary_prompt`).

use serde::{Deserialize, Serialize};

use super::{AgentEvent, Attachment, TaskId};
use crate::tr;

/// Where a task came from: the conversation it continues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskOrigin {
    pub conversation: TaskId,
    /// The conversation's title when it was carried: the link still reads
    /// well after the conversation is deleted.
    pub title: String,
    /// The context section that opens the task's prompt, exactly as the
    /// agent got it (the user's instruction follows it).
    pub context: String,
}

/// One question of a conversation with the agent's final answer to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextTurn {
    pub question: String,
    pub answer: String,
    /// Images attached to the question.
    #[serde(default)]
    pub images: Vec<Attachment>,
}

/// From this many characters a context is long: the UI notes its size and
/// suggests "Resumir con IA".
pub const LONG_CONTEXT_CHARS: usize = 12_000;

/// The questions and answers sent to "Resumir con IA" are cut here
/// (characters, the most recent kept).
pub const SUMMARY_INPUT_MAX: usize = 40_000;

/// The conversation's turns: each question with the agent's final answer
/// (the turn's `Finished` summary, else its last message). A question
/// without an answer (failed, stopped before saying anything, or still
/// running) is left out.
pub fn conversation_turns(events: &[AgentEvent]) -> Vec<ContextTurn> {
    struct Open {
        question: String,
        images: Vec<Attachment>,
        last_message: Option<String>,
        answer: Option<String>,
    }
    fn close(open: Option<Open>, turns: &mut Vec<ContextTurn>) {
        let Some(open) = open else { return };
        let answer = open.answer.or(open.last_message).unwrap_or_default();
        if !answer.trim().is_empty() && !open.question.trim().is_empty() {
            turns.push(ContextTurn {
                question: open.question.trim().to_owned(),
                answer: answer.trim().to_owned(),
                images: open.images,
            });
        }
    }
    let mut turns = Vec::new();
    let mut open: Option<Open> = None;
    for event in events {
        match event {
            AgentEvent::UserMessage { text, images } => {
                close(open.take(), &mut turns);
                open = Some(Open {
                    question: text.clone(),
                    images: images.clone(),
                    last_message: None,
                    answer: None,
                });
            }
            AgentEvent::Message { text } => {
                if let Some(open) = open.as_mut()
                    && !text.trim().is_empty()
                {
                    open.last_message = Some(text.clone());
                }
            }
            AgentEvent::Finished { ok, summary, .. } => {
                if let Some(open) = open.as_mut()
                    && *ok
                    && !summary.trim().is_empty()
                {
                    open.answer = Some(summary.clone());
                }
            }
            _ => {}
        }
    }
    close(open, &mut turns);
    turns
}

/// Characters of the questions and answers, to tell a long context.
pub fn context_chars(turns: &[ContextTurn]) -> usize {
    turns
        .iter()
        .map(|t| t.question.chars().count() + t.answer.chars().count())
        .sum()
}

/// What opens the task's prompt: a delimited section with the questions
/// and answers (or a summary of them), then the user's instruction follows.
/// `images_attached`: the questions' images go with the task's message.
pub fn context_section(
    title: &str,
    turns: &[ContextTurn],
    summary: Option<&str>,
    images_attached: bool,
) -> String {
    let mut out = tr!(
        "--- Contexto: conversación «{title}» ---",
        "--- Context: conversation «{title}» ---"
    );
    out.push('\n');
    out.push_str(&tr!(
        "Viene de una pregunta rápida hecha en Hedy fuera de este proyecto. Úsala como contexto; la instrucción va después de este bloque.",
        "It comes from a quick question asked in Hedy outside this project. Use it as context; the instruction follows this block."
    ));
    out.push_str("\n\n");
    match summary.map(str::trim).filter(|s| !s.is_empty()) {
        Some(summary) => {
            out.push_str(&tr!(
                "Resumen de la conversación (hecho con IA):",
                "Summary of the conversation (made with AI):"
            ));
            out.push('\n');
            out.push_str(summary);
            out.push('\n');
        }
        None => {
            for (index, turn) in turns.iter().enumerate() {
                let n = index + 1;
                out.push_str(&tr!("Pregunta {n}:", "Question {n}:"));
                out.push('\n');
                out.push_str(&turn.question);
                out.push('\n');
                if !turn.images.is_empty() {
                    let names = turn
                        .images
                        .iter()
                        .map(|i| i.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&tr!("(Imágenes: {names})", "(Images: {names})"));
                    out.push('\n');
                }
                out.push('\n');
                out.push_str(&tr!("Respuesta {n}:", "Answer {n}:"));
                out.push('\n');
                out.push_str(&turn.answer);
                out.push_str("\n\n");
            }
        }
    }
    let images = turns.iter().map(|t| t.images.len()).sum::<usize>();
    if images > 0 {
        out.push_str(&if images_attached {
            tr!(
                "Las imágenes de la conversación van adjuntas a este mensaje.",
                "The conversation's images are attached to this message."
            )
        } else {
            tr!(
                "Las imágenes de la conversación no van adjuntas: solo se nombran.",
                "The conversation's images are not attached: they are only named."
            )
        });
        out.push('\n');
    }
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
    out.push_str(&tr!("--- Fin del contexto ---", "--- End of context ---"));
    out
}

/// The task's prompt: the context section, then the instruction.
pub fn carried_prompt(context: &str, instruction: &str) -> String {
    format!("{context}\n\n{}", instruction.trim())
}

/// What "Resumir con IA" asks the conversation's own agent: a summary to
/// carry as context, from the questions and answers (the most recent kept
/// when they are too long).
pub fn summary_prompt(title: &str, turns: &[ContextTurn]) -> String {
    let mut body = String::new();
    for (index, turn) in turns.iter().enumerate() {
        let n = index + 1;
        body.push_str(&tr!("Pregunta {n}:", "Question {n}:"));
        body.push('\n');
        body.push_str(&turn.question);
        body.push_str("\n\n");
        body.push_str(&tr!("Respuesta {n}:", "Answer {n}:"));
        body.push('\n');
        body.push_str(&turn.answer);
        body.push_str("\n\n");
    }
    let count = body.chars().count();
    if count > SUMMARY_INPUT_MAX {
        let tail: String = body.chars().skip(count - SUMMARY_INPUT_MAX).collect();
        body = format!("…{tail}");
    }
    let mut out = tr!(
        "Resume esta conversación («{title}») para pasarla como contexto a una tarea de programación. Conserva las conclusiones, las decisiones, los datos concretos (nombres, versiones, comandos, enlaces) y las preguntas abiertas; deja fuera lo accesorio. Responde solo con el resumen en Markdown, sin introducción, en el idioma de la conversación. No uses herramientas.",
        "Summarize this conversation («{title}») to carry it as context into a programming task. Keep the conclusions, the decisions, the concrete facts (names, versions, commands, links) and the open questions; leave out the rest. Reply with the summary only, in Markdown, with no introduction, in the conversation's language. Do not use tools."
    );
    out.push_str("\n\n");
    out.push_str(body.trim_end());
    out
}

/// Cleans the agent's summary: no surrounding fence, no label line.
pub fn clean_summary(raw: &str) -> Option<String> {
    let mut text = raw.trim();
    if let Some(inner) = text.strip_prefix("```") {
        let inner = inner.split_once('\n').map_or("", |(_, rest)| rest);
        text = inner.trim_end().strip_suffix("```").unwrap_or(inner).trim();
    }
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Words left out of a suggested project name.
const NAME_STOPWORDS: &[&str] = &[
    "a", "al", "an", "and", "are", "can", "como", "con", "cual", "cuales", "de", "del", "do",
    "does", "el", "en", "es", "for", "how", "i", "in", "is", "la", "las", "lo", "los", "me", "mi",
    "my", "o", "of", "or", "para", "por", "que", "se", "should", "su", "the", "to", "un", "una",
    "uno", "unos", "what", "which", "y",
];

/// A project name from a conversation's title, already a safe folder name:
/// "¿Qué base de datos uso para un SaaS de citas?" → `base-datos-uso-saas`.
pub fn suggested_project_name(title: &str) -> String {
    const WORDS: usize = 4;
    let slug = super::project::folder_name(title).unwrap_or_default();
    let words: Vec<&str> = slug
        .split(['-', '.'])
        .filter(|w| !w.is_empty() && !NAME_STOPWORDS.contains(w))
        .take(WORDS)
        .collect();
    if !words.is_empty() {
        return words.join("-");
    }
    if !slug.is_empty() {
        return slug.split('-').take(WORDS).collect::<Vec<_>>().join("-");
    }
    tr!("proyecto", "project")
}

/// "Guardar como nota del proyecto": the title on the first line (what
/// agents see first in `memory_search`), then the text.
pub fn note_text(title: &str, body: &str) -> String {
    match (title.trim(), body.trim()) {
        ("", body) => body.to_owned(),
        (title, "") => title.to_owned(),
        (title, body) => format!("{title}\n\n{body}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ImageType;
    use crate::i18n::{Language, with_language};

    fn finished(summary: &str) -> AgentEvent {
        AgentEvent::Finished {
            ok: true,
            summary: summary.into(),
            duration_ms: None,
        }
    }

    fn message(text: &str) -> AgentEvent {
        AgentEvent::Message { text: text.into() }
    }

    #[test]
    fn turns_pair_each_question_with_its_final_answer() {
        let image = Attachment {
            name: "captura.png".into(),
            file: "1-0.png".into(),
            media_type: ImageType::Png,
        };
        let events = vec![
            AgentEvent::UserMessage {
                text: " ¿Qué es un monad? ".into(),
                images: vec![image.clone()],
            },
            message("Déjame pensar…"),
            message("Un patrón para encadenar cálculos."),
            finished("Un patrón para encadenar cálculos."),
            AgentEvent::user("¿Y un functor?"),
            message("Algo que se puede mapear."),
            // No summary: the last message is the answer.
            finished(""),
            AgentEvent::user("¿Falló?"),
            AgentEvent::Error {
                message: "boom".into(),
            },
            AgentEvent::Finished {
                ok: false,
                summary: "".into(),
                duration_ms: None,
            },
            AgentEvent::user("¿Sigues?"),
        ];
        let turns = conversation_turns(&events);
        assert_eq!(
            turns,
            vec![
                ContextTurn {
                    question: "¿Qué es un monad?".into(),
                    answer: "Un patrón para encadenar cálculos.".into(),
                    images: vec![image],
                },
                ContextTurn {
                    question: "¿Y un functor?".into(),
                    answer: "Algo que se puede mapear.".into(),
                    images: vec![],
                },
            ]
        );
        assert_eq!(context_chars(&turns[1..]), 14 + 25);
    }

    #[test]
    fn the_context_section_is_delimited_and_names_images() {
        let turns = vec![ContextTurn {
            question: "¿Postgres o SQLite?".into(),
            answer: "SQLite para empezar.".into(),
            images: vec![Attachment {
                name: "diagrama.png".into(),
                file: "1-0.png".into(),
                media_type: ImageType::Png,
            }],
        }];
        let section = context_section("Base de datos", &turns, None, false);
        assert!(section.starts_with("--- Contexto: conversación «Base de datos» ---\n"));
        assert!(section.ends_with("\n--- Fin del contexto ---"));
        assert!(section.contains("Pregunta 1:\n¿Postgres o SQLite?\n(Imágenes: diagrama.png)"));
        assert!(section.contains("Respuesta 1:\nSQLite para empezar."));
        assert!(section.contains("no van adjuntas"));
        assert!(context_section("x", &turns, None, true).contains("van adjuntas a este mensaje"));

        let prompt = carried_prompt(&section, "  Crea el esquema  ");
        assert!(prompt.ends_with("--- Fin del contexto ---\n\nCrea el esquema"));

        let summarized = context_section("Base de datos", &turns, Some(" Usar SQLite. "), true);
        assert!(summarized.contains("hecho con IA):\nUsar SQLite.\n"));
        assert!(!summarized.contains("Pregunta 1"));

        let english = with_language(Language::English, || {
            context_section("DB", &turns, None, true)
        });
        assert!(english.starts_with("--- Context: conversation «DB» ---"));
        assert!(english.ends_with("--- End of context ---"));
    }

    #[test]
    fn the_summary_prompt_keeps_the_most_recent_part() {
        let turns = vec![
            ContextTurn {
                question: "primera".into(),
                answer: "x".repeat(SUMMARY_INPUT_MAX),
                images: vec![],
            },
            ContextTurn {
                question: "última".into(),
                answer: "fin".into(),
                images: vec![],
            },
        ];
        let prompt = summary_prompt("T", &turns);
        assert!(prompt.starts_with("Resume esta conversación («T»)"));
        assert!(prompt.contains("Pregunta 2:\núltima"));
        assert!(!prompt.contains("primera"));
        assert!(prompt.chars().count() < SUMMARY_INPUT_MAX + 1000);
    }

    #[test]
    fn summaries_lose_their_fences() {
        assert_eq!(
            clean_summary("```markdown\n- Usar SQLite\n```"),
            Some("- Usar SQLite".into())
        );
        assert_eq!(clean_summary("  - A  "), Some("- A".into()));
        assert_eq!(clean_summary("   "), None);
    }

    #[test]
    fn project_names_are_short_safe_folders() {
        assert_eq!(
            suggested_project_name("¿Qué base de datos uso para un SaaS de citas?"),
            "base-datos-uso-saas"
        );
        assert_eq!(
            suggested_project_name("How do I parse TOML in Rust"),
            "parse-toml-rust"
        );
        assert_eq!(suggested_project_name("¿Qué es?"), "que-es");
        assert_eq!(suggested_project_name("¿¿??"), "proyecto");
    }

    #[test]
    fn note_text_puts_the_title_first() {
        assert_eq!(note_text(" Base ", " SQLite "), "Base\n\nSQLite");
        assert_eq!(note_text("", "SQLite"), "SQLite");
        assert_eq!(note_text("Base", ""), "Base");
    }
}
