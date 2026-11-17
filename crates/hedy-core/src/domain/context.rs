//! The fixed context Hedy adds to every agent session that reads the
//! project's memory, before the agent does anything: the guidance appended to
//! its system prompt (`prompts::memory_guidance`) and the definitions of the
//! memory and skill tools the `hedy-memory` server registers.
//!
//! The tool definitions live here so the memory MCP server, the API agent and
//! the overhead estimate use the same text. Two catalogs:
//! - **today's** ([`LEGACY_TOOLS`]): exactly what the server has always
//!   served (Spanish descriptions, schemas as `schemars` writes them);
//! - **trimmed** (the `trim-tools` experiment, [`TRIMMED_TOOLS`]): English
//!   (models read it, not the user; it tokenizes shorter) and compact schemas.
//!
//! The `search-8` experiment adds `limit` / `offset` to `memory_search` and
//! `skill_list` in either catalog.
//!
//! Tokens are an **estimate**: characters / 4, rounded up ([`estimate_tokens`]).
//! It is the usual rule of thumb for English with the providers' tokenizers;
//! Spanish runs a bit higher, and each CLI wraps tool definitions in its own
//! framing (Claude Code also prefixes `mcp__hedy-memory__` to each name), so
//! read the figures as ±25 %: good for comparing versions, not for billing.

use serde_json::{Map, Value, json};

use super::{Phase, TaskMode, prompts};
use crate::experiments::{self, Experiment, Experiments};

/// Characters per token of the estimate.
pub const CHARS_PER_TOKEN: usize = 4;

/// Rough token count of `text`: characters / 4, rounded up. No tokenizer, no
/// network.
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(CHARS_PER_TOKEN)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    String,
    Integer,
}

/// One parameter of a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamDoc {
    pub name: &'static str,
    pub kind: ParamKind,
    /// Empty: none (the name says it all).
    pub description: &'static str,
    pub required: bool,
}

/// One tool of the trimmed catalog: name, description and parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolDoc {
    pub name: &'static str,
    pub description: &'static str,
    pub params: &'static [ParamDoc],
}

const fn param(
    name: &'static str,
    kind: ParamKind,
    description: &'static str,
    required: bool,
) -> ParamDoc {
    ParamDoc {
        name,
        kind,
        description,
        required,
    }
}

/// The paging parameters of `search-8`.
pub const NOTES_LIMIT: &str = "Notes to return (default 8, max 30).";
pub const SKILLS_LIMIT: &str = "Skills to return (default 20, max 50).";
pub const PAGE_OFFSET: &str = "Results to skip, to see more.";

pub const TRIMMED_TOOLS: [ToolDoc; 7] = [
    ToolDoc {
        name: "memory_search",
        description: "Search the project's approved notes (decisions, conventions, rules). One line per note, best first; memory_get returns a whole note.",
        params: &[param(
            "query",
            ParamKind::String,
            "Words to look for; empty lists every note.",
            false,
        )],
    },
    ToolDoc {
        name: "memory_get",
        description: "One whole approved note, by id or topic.",
        params: &[
            param("id", ParamKind::Integer, "From memory_search.", false),
            param(
                "topic",
                ParamKind::String,
                "Or the topic, e.g. arquitectura/pagos.",
                false,
            ),
        ],
    },
    ToolDoc {
        name: "memory_topics",
        description: "The memory's topics, each with its note.",
        params: &[],
    },
    ToolDoc {
        name: "memory_propose",
        description: "Propose a note (a lasting decision or convention); the user approves it. Under an existing topic it proposes a change to that note.",
        params: &[
            param("text", ParamKind::String, "One sentence.", true),
            param(
                "category",
                ParamKind::String,
                "decision, convention, rule or preference.",
                true,
            ),
            param(
                "topic",
                ParamKind::String,
                "Optional lowercase path, e.g. arquitectura/pagos.",
                false,
            ),
        ],
    },
    ToolDoc {
        name: "skill_list",
        description: "The approved skills (reusable procedures): name and when to use each.",
        params: &[],
    },
    ToolDoc {
        name: "skill_get",
        description: "One whole approved skill (its steps), by name.",
        params: &[param("name", ParamKind::String, "From skill_list.", true)],
    },
    ToolDoc {
        name: "skill_propose",
        description: "Propose a skill: a non-obvious, reusable procedure; the user approves it. An existing name proposes its update.",
        params: &[
            param(
                "name",
                ParamKind::String,
                "Lowercase with dashes, e.g. run-e2e-tests.",
                true,
            ),
            param(
                "description",
                ParamKind::String,
                "One line: when to use it.",
                true,
            ),
            param(
                "body",
                ParamKind::String,
                "The steps, in Markdown. No secrets.",
                true,
            ),
        ],
    },
];

/// Today's tool definitions, exactly as the memory server has served them
/// (rmcp + schemars output, measured on Nov 16, 2026): the default until the
/// `trim-tools` experiment is measured.
pub const LEGACY_TOOLS: &str = r##"[
{"name":"memory_search","description":"Busca en la memoria del proyecto: decisiones, convenciones, reglas y preferencias aprobadas por el usuario. Devuelve un resumen por nota (id, categoría, tema y el comienzo del texto); memory_get devuelve una completa. Sin texto devuelve todas.","input_schema":{"$schema":"https://json-schema.org/draft/2020-12/schema","properties":{"query":{"default":"","description":"Words to look for. Empty: every note.","type":"string"}},"type":"object"}},
{"name":"memory_get","description":"Devuelve una nota completa de la memoria del proyecto, por id (de memory_search) o por tema.","input_schema":{"$schema":"https://json-schema.org/draft/2020-12/schema","properties":{"id":{"default":null,"description":"The note's id, from memory_search.","format":"int64","type":["integer","null"]},"topic":{"default":null,"description":"Or its topic, such as arquitectura/pagos.","type":["string","null"]}},"type":"object"}},
{"name":"memory_topics","description":"Lista los temas de la memoria del proyecto (p. ej. arquitectura/pagos) con la nota de cada uno, para proponer cambios bajo el mismo tema.","input_schema":{"properties":{},"type":"object"}},
{"name":"memory_propose","description":"Propone una nota para la memoria del proyecto (una decisión duradera o una convención). No se guarda hasta que el usuario la apruebe. Con el tema de una nota existente, propone un cambio de esa nota.","input_schema":{"$schema":"https://json-schema.org/draft/2020-12/schema","properties":{"category":{"description":"decision, convention, rule or preference.","type":"string"},"text":{"description":"The note, in one sentence.","type":"string"},"topic":{"default":null,"description":"Optional stable topic, a lowercase path such as arquitectura/pagos.\nUnder the topic of an existing note, the proposal is a change to it.","type":["string","null"]}},"required":["text","category"],"type":"object"}},
{"name":"skill_list","description":"Lista las habilidades aprobadas del proyecto: procedimientos reutilizables (correr los tests e2e, publicar una versión…), con el nombre y cuándo usar cada una. skill_get devuelve una completa.","input_schema":{"properties":{},"type":"object"}},
{"name":"skill_get","description":"Devuelve una habilidad aprobada completa (sus pasos), por nombre.","input_schema":{"$schema":"https://json-schema.org/draft/2020-12/schema","properties":{"name":{"description":"The skill's name, from skill_list.","type":"string"}},"required":["name"],"type":"object"}},
{"name":"skill_propose","description":"Propone una habilidad: un procedimiento reutilizable que no es obvio, descubierto en una tarea que salió bien. No se guarda hasta que el usuario la apruebe. Con el nombre de una existente, propone su actualización. Nunca incluyas secretos.","input_schema":{"$schema":"https://json-schema.org/draft/2020-12/schema","properties":{"body":{"description":"The steps, in Markdown. No secrets, tokens or .env values.","type":"string"},"description":{"description":"One line: when to use it.","type":"string"},"name":{"description":"Short lowercase name with dashes, such as run-e2e-tests. With the\nname of an existing skill, the proposal is an update of it.","type":"string"}},"required":["name","description","body"],"type":"object"}}
]"##;

impl ToolDoc {
    /// The parameters as a JSON schema object.
    pub fn input_schema(&self) -> Value {
        let mut properties = Map::new();
        for p in self.params {
            properties.insert(p.name.into(), property(p.kind, p.description));
        }
        let required: Vec<&str> = self
            .params
            .iter()
            .filter(|p| p.required)
            .map(|p| p.name)
            .collect();
        let mut schema = json!({"type": "object", "properties": properties});
        if !required.is_empty() {
            schema["required"] = json!(required);
        }
        schema
    }

    /// The definition as tool-calling APIs take it: name, description and
    /// input schema.
    pub fn definition(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "input_schema": self.input_schema(),
        })
    }
}

fn property(kind: ParamKind, description: &str) -> Value {
    let kind = match kind {
        ParamKind::String => "string",
        ParamKind::Integer => "integer",
    };
    let mut schema = json!({"type": kind});
    if !description.is_empty() {
        schema["description"] = json!(description);
    }
    schema
}

/// The memory and skill tools a session gets with `set`, in order, as
/// `{name, description, input_schema}`.
pub fn tool_definitions(set: Experiments) -> Vec<Value> {
    let mut tools: Vec<Value> = if set.has(Experiment::TrimTools) {
        TRIMMED_TOOLS.iter().map(ToolDoc::definition).collect()
    } else {
        serde_json::from_str(LEGACY_TOOLS).expect("LEGACY_TOOLS is valid JSON")
    };
    if set.has(Experiment::Search8) {
        for tool in &mut tools {
            let limit = match tool["name"].as_str() {
                Some("memory_search") => NOTES_LIMIT,
                Some("skill_list") => SKILLS_LIMIT,
                _ => continue,
            };
            if let Some(props) = tool["input_schema"]["properties"].as_object_mut() {
                props.insert("limit".into(), property(ParamKind::Integer, limit));
                props.insert("offset".into(), property(ParamKind::Integer, PAGE_OFFSET));
            }
        }
    }
    tools
}

/// One tool for this process's experiments: its description and input
/// schema, for the memory server. Panics on an unknown name (a typo in Hedy).
pub fn tool_definition(name: &str) -> Value {
    tool_definitions(experiments::current())
        .into_iter()
        .find(|t| t["name"] == name)
        .unwrap_or_else(|| panic!("unknown Hedy tool {name}"))
}

/// What Hedy adds to one session that reads the memory, in characters and
/// estimated tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionOverhead {
    pub phase: Phase,
    pub mode: TaskMode,
    pub guidance_chars: usize,
    pub tools_chars: usize,
}

impl SessionOverhead {
    pub fn guidance_tokens(&self) -> usize {
        self.guidance_chars.div_ceil(CHARS_PER_TOKEN)
    }

    pub fn tools_tokens(&self) -> usize {
        self.tools_chars.div_ceil(CHARS_PER_TOKEN)
    }

    pub fn total_tokens(&self) -> usize {
        self.guidance_tokens() + self.tools_tokens()
    }
}

/// The overhead of one session on `phase` in `mode` with `set`, in the
/// current language.
pub fn session_overhead(phase: Phase, mode: TaskMode, set: Experiments) -> SessionOverhead {
    let tools: usize = tool_definitions(set)
        .iter()
        .map(|t| t.to_string().chars().count())
        .sum();
    SessionOverhead {
        phase,
        mode,
        guidance_chars: prompts::memory_guidance_in(phase, mode, set)
            .chars()
            .count(),
        tools_chars: tools,
    }
}

/// Every phase and mode a task can run in: the project's four phases, then a
/// question and a research (a single stage each).
pub const SESSION_KINDS: [(Phase, TaskMode); 6] = [
    (Phase::Plan, TaskMode::Project),
    (Phase::Implement, TaskMode::Project),
    (Phase::Tests, TaskMode::Project),
    (Phase::Review, TaskMode::Project),
    (Phase::Implement, TaskMode::Ask),
    (Phase::Implement, TaskMode::Research),
];

pub fn session_overheads(set: Experiments) -> Vec<SessionOverhead> {
    SESSION_KINDS
        .into_iter()
        .map(|(phase, mode)| session_overhead(phase, mode, set))
        .collect()
}

/// The experiments that change the fixed overhead.
pub fn trimmed() -> Experiments {
    Experiments::NONE
        .with(Experiment::TrimGuidance)
        .with(Experiment::TrimTools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, with_language};

    /// Estimated tokens Hedy may add to a session, at most, per catalog.
    /// Measured on Nov 16, 2026 (docs/decisions.md §33): raise one only on
    /// purpose, so the overhead cannot creep back silently.
    const TODAY_GUIDANCE_BUDGET: usize = 490;
    const TODAY_TOOLS_BUDGET: usize = 815;
    const TRIMMED_GUIDANCE_BUDGET: usize = 250;
    const TRIMMED_TOOLS_BUDGET: usize = 520;
    /// `search-8` adds the paging parameters.
    const PAGING_BUDGET: usize = 80;

    #[test]
    fn estimates_a_token_per_four_characters() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        // Characters, not bytes.
        assert_eq!(estimate_tokens("ñañá"), 1);
    }

    fn check(set: Experiments, guidance_budget: usize, tools_budget: usize) {
        for language in [Language::Spanish, Language::English] {
            for o in with_language(language, || session_overheads(set)) {
                assert!(
                    o.guidance_tokens() <= guidance_budget,
                    "{set:?} {language:?} {:?}/{:?} guidance: {} tokens > {guidance_budget}",
                    o.phase,
                    o.mode,
                    o.guidance_tokens()
                );
                assert!(
                    o.tools_tokens() <= tools_budget,
                    "{set:?} tools: {} tokens > {tools_budget}",
                    o.tools_tokens()
                );
            }
        }
    }

    #[test]
    fn todays_overhead_stays_within_budget() {
        check(Experiments::NONE, TODAY_GUIDANCE_BUDGET, TODAY_TOOLS_BUDGET);
        check(
            Experiments::NONE.with(Experiment::Search8),
            TODAY_GUIDANCE_BUDGET,
            TODAY_TOOLS_BUDGET + PAGING_BUDGET,
        );
    }

    #[test]
    fn the_trimmed_overhead_stays_within_budget_and_halves_the_guidance() {
        check(trimmed(), TRIMMED_GUIDANCE_BUDGET, TRIMMED_TOOLS_BUDGET);
        check(
            Experiments::all(),
            TRIMMED_GUIDANCE_BUDGET,
            TRIMMED_TOOLS_BUDGET + PAGING_BUDGET,
        );
        for (today, trim) in session_overheads(Experiments::NONE)
            .into_iter()
            .zip(session_overheads(trimmed()))
        {
            assert!(
                trim.guidance_chars * 2 <= today.guidance_chars,
                "{today:?} {trim:?}"
            );
            assert!(trim.tools_chars < today.tools_chars);
        }
    }

    #[test]
    fn readers_get_less_guidance_than_writers() {
        for set in [Experiments::NONE, trimmed()] {
            let writer = session_overhead(Phase::Implement, TaskMode::Project, set);
            let reviewer = session_overhead(Phase::Review, TaskMode::Project, set);
            assert!(reviewer.guidance_chars < writer.guidance_chars);
            assert_eq!(reviewer.tools_chars, writer.tools_chars);
            assert_eq!(
                writer.total_tokens(),
                writer.guidance_tokens() + writer.tools_tokens()
            );
        }
    }

    #[test]
    fn both_catalogs_have_the_same_tools_and_required_parameters() {
        let names = |set| -> Vec<String> {
            tool_definitions(set)
                .iter()
                .map(|t| t["name"].as_str().unwrap().to_owned())
                .collect()
        };
        assert_eq!(names(Experiments::NONE), names(trimmed()));
        assert_eq!(names(Experiments::NONE).len(), 7);
        for (today, trim) in tool_definitions(Experiments::NONE)
            .iter()
            .zip(tool_definitions(trimmed()))
        {
            let required = |t: &Value| t["input_schema"]["required"].clone();
            assert_eq!(required(today), required(&trim), "{}", today["name"]);
            let props = |t: &Value| -> Vec<String> {
                let mut keys: Vec<String> = t["input_schema"]["properties"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect();
                keys.sort();
                keys
            };
            assert_eq!(props(today), props(&trim), "{}", today["name"]);
        }
    }

    #[test]
    fn paging_parameters_only_with_search_8() {
        let props = |set, name: &str| {
            tool_definitions(set)
                .into_iter()
                .find(|t| t["name"] == name)
                .unwrap()["input_schema"]["properties"]
                .clone()
        };
        assert!(props(Experiments::NONE, "memory_search")["limit"].is_null());
        let paged = Experiments::NONE.with(Experiment::Search8);
        assert_eq!(props(paged, "memory_search")["limit"]["type"], "integer");
        assert!(props(paged, "skill_list")["offset"].is_object());
        assert!(props(paged, "memory_get")["limit"].is_null());
        assert_eq!(
            tool_definition("memory_topics")["input_schema"],
            json!({"properties": {}, "type": "object"})
        );
    }
}
