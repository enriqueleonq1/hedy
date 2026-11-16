//! Claude Code's stream-json messages → the common event language.
//! Formats verified in phase 0 (docs/decisions.md §3).

use crate::adapters::paths::relative;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::domain::agent::looks_like_tests;
use crate::domain::permissions::summarize;
use crate::domain::{
    AgentCommand, AgentEvent, CommandSource, Decision, PlanStep, StepStatus, UsageSample,
};

const EDIT_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// Stateful: a tool's activity line is emitted when its result arrives, so
/// denied or failed edits do not show up as done.
pub struct Translator {
    cwd: PathBuf,
    /// tool_use_id → (tool, input).
    tool_uses: HashMap<String, (String, Value)>,
    /// The commands in the answer to Hedy's `initialize` request, with their
    /// descriptions; the `init` message only names them.
    described: Option<Vec<Value>>,
    /// The session's commands were reported already.
    commands_sent: bool,
}

/// The id of the `initialize` request Hedy sends first (see `mod.rs`).
pub const INIT_REQUEST_ID: &str = "hedy-init";

impl Translator {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            tool_uses: HashMap::new(),
            described: None,
            commands_sent: false,
        }
    }

    fn relative(&self, path: &str) -> String {
        relative(&self.cwd, Path::new(path))
    }

    /// `denied` holds tool_use ids whose permission the user (or a rule) denied.
    pub fn translate(&mut self, msg: &Value, denied: &HashSet<String>) -> Vec<AgentEvent> {
        let str_of = |v: &Value| v.as_str().unwrap_or_default().to_owned();
        match msg["type"].as_str().unwrap_or("") {
            "system" if msg["subtype"] == "init" => {
                let mut events = vec![AgentEvent::SessionStarted {
                    session_id: str_of(&msg["session_id"]),
                    model: str_of(&msg["model"]),
                }];
                if !self.commands_sent {
                    self.commands_sent = true;
                    events.push(AgentEvent::Commands {
                        commands: commands(self.described.as_deref(), msg),
                    });
                }
                events
            }
            "control_response" if msg["response"]["request_id"] == INIT_REQUEST_ID => {
                if let Some(list) = msg["response"]["response"]["commands"].as_array() {
                    self.described = Some(list.clone());
                }
                vec![]
            }
            "assistant" => self.assistant(&msg["message"]["content"]),
            "user" => self.tool_results(&msg["message"]["content"], denied),
            "control_request" if msg["request"]["subtype"] == "can_use_tool" => {
                let request = &msg["request"];
                let request_id = str_of(&msg["request_id"]);
                let tool = str_of(&request["tool_name"]);
                let input = request["input"].clone();
                if tool == "ExitPlanMode" {
                    vec![AgentEvent::Plan {
                        text: str_of(&input["plan"]),
                        request_id: Some(request_id),
                    }]
                } else {
                    vec![AgentEvent::PermissionRequest {
                        request_id,
                        summary: summarize(&tool, &input),
                        tool,
                        input,
                    }]
                }
            }
            "rate_limit_event" => {
                let info = &msg["rate_limit_info"];
                vec![AgentEvent::RateLimit {
                    status: info["status"].as_str().unwrap_or("unknown").to_owned(),
                    // Claude reports 0–1; Hedy uses percentages.
                    utilization: info["utilization"]
                        .as_f64()
                        .map(|u| if u <= 1.0 { u * 100.0 } else { u }),
                    resets_at: info["resetsAt"].as_i64(),
                }]
            }
            "result" => {
                let usage = &msg["usage"];
                let tokens = |key: &str| usage[key].as_u64().unwrap_or(0);
                vec![
                    AgentEvent::Usage(UsageSample {
                        input_tokens: tokens("input_tokens"),
                        output_tokens: tokens("output_tokens"),
                        cache_read_tokens: tokens("cache_read_input_tokens"),
                        cache_write_tokens: tokens("cache_creation_input_tokens"),
                        cost_usd: msg["total_cost_usd"].as_f64(),
                    }),
                    AgentEvent::Finished {
                        ok: !msg["is_error"].as_bool().unwrap_or(false),
                        summary: str_of(&msg["result"]),
                        duration_ms: msg["duration_ms"].as_u64(),
                    },
                ]
            }
            _ => vec![],
        }
    }

    fn assistant(&mut self, content: &Value) -> Vec<AgentEvent> {
        let mut events = Vec::new();
        for block in content.as_array().into_iter().flatten() {
            match block["type"].as_str() {
                Some("text") => {
                    let text = block["text"].as_str().unwrap_or("").trim();
                    if !text.is_empty() {
                        events.push(AgentEvent::Message {
                            text: text.to_owned(),
                        });
                    }
                }
                Some("tool_use") => {
                    let tool = block["name"].as_str().unwrap_or("").to_owned();
                    let input = block["input"].clone();
                    if tool == "TodoWrite" {
                        events.push(AgentEvent::Steps {
                            steps: steps(&input),
                        });
                    } else if tool != "ExitPlanMode" {
                        let detail = match input["command"].as_str() {
                            Some(command) if tool == "Bash" => command.to_owned(),
                            _ => self.relative(&summarize(&tool, &input)),
                        };
                        events.push(AgentEvent::ToolStarted {
                            tool: tool.clone(),
                            detail,
                        });
                        let id = block["id"].as_str().unwrap_or("").to_owned();
                        self.tool_uses.insert(id, (tool, input));
                    }
                }
                _ => {}
            }
        }
        events
    }

    fn tool_results(&mut self, content: &Value, denied: &HashSet<String>) -> Vec<AgentEvent> {
        let mut events = Vec::new();
        for block in content.as_array().into_iter().flatten() {
            if block["type"] != "tool_result" {
                continue;
            }
            let id = block["tool_use_id"].as_str().unwrap_or("");
            let Some((tool, input)) = self.tool_uses.remove(id) else {
                continue;
            };
            if denied.contains(id) {
                continue;
            }
            let failed = block["is_error"].as_bool().unwrap_or(false);
            // Blocked by Hedy's PreToolUse hook: report the block, not a run.
            let text = result_text(&block["content"]);
            if failed && HOOK_MARKS.iter().any(|mark| text.contains(mark)) {
                events.push(AgentEvent::PermissionResolved {
                    request_id: format!("hook-{id}"),
                    decision: Decision::Deny,
                    automatic: true,
                    reason: Some(text),
                });
                continue;
            }
            let event = match tool.as_str() {
                t if EDIT_TOOLS.contains(&t) => {
                    let path = summarize(t, &input);
                    // Plan mode saves its plan under ~/.claude/plans without asking;
                    // the plan itself arrives through ExitPlanMode.
                    let cli_file =
                        !Path::new(&path).starts_with(&self.cwd) && path.contains("/.claude/");
                    if failed || cli_file {
                        continue;
                    }
                    AgentEvent::FileEdited {
                        path: self.relative(&path),
                    }
                }
                "Bash" => {
                    let command = input["command"].as_str().unwrap_or("").to_owned();
                    if looks_like_tests(&command) {
                        AgentEvent::TestsRun {
                            command,
                            passed: !failed,
                        }
                    } else {
                        AgentEvent::CommandRun {
                            command,
                            tests: false,
                        }
                    }
                }
                // A blocked or failed read did not happen; do not report it as done.
                _ if failed => continue,
                _ => AgentEvent::ToolUsed {
                    summary: self.relative(&summarize(&tool, &input)),
                    tool,
                },
            };
            events.push(event);
        }
        events
    }
}

/// The session's slash commands (verified with Claude Code 2.1.306, §32).
/// `described`: the `commands` of the `initialize` answer (`name`,
/// `description`, `argumentHint`, `builtin`; the user's own say "(user)" or
/// "(project)" at the end, a plugin's start with "(plugin)"). `init` names
/// them all in `slash_commands`, and says which are `skills`, which
/// `plugins` are loaded and which commands only work in a terminal
/// (`terminal_slash_commands`, left out).
fn commands(described: Option<&[Value]>, init: &Value) -> Vec<AgentCommand> {
    let names = |key: &str| -> HashSet<String> {
        init[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    };
    let skills = names("skills");
    let terminal = names("terminal_slash_commands");
    let plugins: HashSet<String> = init["plugins"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["name"].as_str().map(str::to_owned))
        .collect();
    let plugin_of = |name: &str, description: &str| {
        name.split_once(':')
            .map(|(prefix, _)| prefix)
            .filter(|p| plugins.contains(*p) || description.starts_with(&format!("({p})")))
            .map(str::to_owned)
    };
    let source = |name: &str, description: &str, builtin: Option<bool>| {
        if plugin_of(name, description).is_some() {
            CommandSource::Plugin
        } else if builtin == Some(true) {
            CommandSource::Builtin
        } else if skills.contains(name) {
            CommandSource::Skill
        } else if builtin.is_some() && !name.starts_with("mcp__") {
            CommandSource::Custom
        } else {
            CommandSource::Unknown
        }
    };
    let text = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_owned());
    let mut found: Vec<AgentCommand> = Vec::new();
    for entry in described.into_iter().flatten() {
        let name = entry["name"].as_str().unwrap_or("").to_owned();
        if name.is_empty() || terminal.contains(&name) {
            continue;
        }
        let raw = entry["description"].as_str().unwrap_or("").trim();
        let builtin = Some(entry["builtin"].as_bool().unwrap_or(false));
        let mut description = raw;
        for scope in [" (user)", " (project)"] {
            description = description.strip_suffix(scope).unwrap_or(description);
        }
        let plugin = plugin_of(&name, raw);
        let tag = plugin.map(|p| format!("({p})")).unwrap_or_default();
        if !tag.is_empty() {
            description = description.strip_prefix(&tag).unwrap_or(description);
        }
        found.push(AgentCommand {
            description: text(description),
            argument_hint: entry["argumentHint"].as_str().and_then(text),
            source: source(&name, raw, builtin),
            name,
        });
    }
    // Named in `init` only (an older CLI, or MCP prompts that came later).
    for name in init["slash_commands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !terminal.contains(name) && !found.iter().any(|c| c.name == name) {
            found.push(AgentCommand::new(name, source(name, "", None)));
        }
    }
    found
}

/// Every reason Hedy gives starts with one of these (see `domain::permissions`),
/// in either language.
const HOOK_MARKS: [&str; 2] = ["Hedy no permite", "Hedy does not allow"];

/// A tool result's content: a string or a list of text blocks.
fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn steps(input: &Value) -> Vec<PlanStep> {
    input["todos"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|todo| PlanStep {
            text: todo["content"].as_str().unwrap_or("").to_owned(),
            status: match todo["status"].as_str() {
                Some("completed") => StepStatus::Done,
                Some("in_progress") => StepStatus::InProgress,
                _ => StepStatus::Pending,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn translator() -> Translator {
        Translator::new("/wt".into())
    }

    #[test]
    fn session_messages_and_steps() {
        let mut t = translator();
        let none = HashSet::new();
        assert_eq!(
            t.translate(&json!({"type":"system","subtype":"init","session_id":"s1","model":"claude-opus-5-5"}), &none),
            vec![
                AgentEvent::SessionStarted { session_id: "s1".into(), model: "claude-opus-5-5".into() },
                AgentEvent::Commands { commands: vec![] },
            ]
        );
        // The commands go once per session.
        assert_eq!(
            t.translate(
                &json!({"type":"system","subtype":"init","session_id":"s1","model":"m"}),
                &none
            )
            .len(),
            1
        );
        let assistant = json!({"type":"assistant","message":{"content":[
            {"type":"text","text":"Voy a buscar."},
            {"type":"tool_use","id":"tu0","name":"TodoWrite","input":{"todos":[
                {"content":"Buscar","status":"completed","activeForm":"Buscando"},
                {"content":"Correr tests","status":"in_progress","activeForm":"Corriendo"},
                {"content":"Commit","status":"pending","activeForm":"x"}]}}
        ]}});
        let events = t.translate(&assistant, &none);
        assert_eq!(
            events[0],
            AgentEvent::Message {
                text: "Voy a buscar.".into()
            }
        );
        let AgentEvent::Steps { steps } = &events[1] else {
            panic!("{events:?}")
        };
        let statuses: Vec<_> = steps.iter().map(|s| s.status).collect();
        assert_eq!(
            statuses,
            vec![
                StepStatus::Done,
                StepStatus::InProgress,
                StepStatus::Pending
            ]
        );
    }

    /// As Claude Code 2.1.306 answers (trimmed): `initialize` first, then
    /// the `init` of the first turn.
    #[test]
    fn commands_come_from_the_initialize_answer_and_the_init_message() {
        let mut t = translator();
        let none = HashSet::new();
        let answer = json!({"type":"control_response","response":{"subtype":"success","request_id":"hedy-init",
        "response":{"commands":[
            {"name":"hello","description":"Say hello to someone (user)","argumentHint":"<name>"},
            {"name":"projcmd","description":"Project command (project)","argumentHint":""},
            {"name":"demo-skill","description":"A demo skill (user)","argumentHint":""},
            {"name":"studio:mockup","description":"(studio) Draw a mockup","argumentHint":"<screen>"},
            {"name":"studio:brand","description":"(studio) Apply the brand","argumentHint":"","aliases":["brand"]},
            {"name":"code-review","description":"Review the diff","argumentHint":"[low|high]","aliases":["review"],"builtin":true},
            {"name":"color","description":"Set the prompt bar color","argumentHint":"[red|blue]","builtin":true}
        ]}}});
        assert!(t.translate(&answer, &none).is_empty());
        let init = json!({"type":"system","subtype":"init","session_id":"s1","model":"m",
            "slash_commands":["hello","projcmd","demo-skill","studio:mockup","studio:brand","code-review","color","mcp__docs__search"],
            "skills":["demo-skill","studio:brand","code-review"],
            "plugins":[{"name":"studio","path":"/p","source":"studio@inline"}],
            "terminal_slash_commands":["color"]});
        let events = t.translate(&init, &none);
        let AgentEvent::Commands { commands } = &events[1] else {
            panic!("{events:?}")
        };
        let summary: Vec<(&str, CommandSource, Option<&str>, Option<&str>)> = commands
            .iter()
            .map(|c| {
                (
                    c.name.as_str(),
                    c.source,
                    c.description.as_deref(),
                    c.argument_hint.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "hello",
                    CommandSource::Custom,
                    Some("Say hello to someone"),
                    Some("<name>")
                ),
                (
                    "projcmd",
                    CommandSource::Custom,
                    Some("Project command"),
                    None
                ),
                (
                    "demo-skill",
                    CommandSource::Skill,
                    Some("A demo skill"),
                    None
                ),
                (
                    "studio:mockup",
                    CommandSource::Plugin,
                    Some("Draw a mockup"),
                    Some("<screen>")
                ),
                (
                    "studio:brand",
                    CommandSource::Plugin,
                    Some("Apply the brand"),
                    None
                ),
                (
                    "code-review",
                    CommandSource::Builtin,
                    Some("Review the diff"),
                    Some("[low|high]")
                ),
                ("mcp__docs__search", CommandSource::Unknown, None, None),
            ]
        );
    }

    #[test]
    fn without_the_initialize_answer_the_init_names_are_enough() {
        let mut t = translator();
        let init = json!({"type":"system","subtype":"init","session_id":"s1","model":"m",
            "slash_commands":["design","studio:mockup","compact"],"skills":["design"],
            "plugins":[{"name":"studio"}]});
        let events = t.translate(&init, &HashSet::new());
        assert_eq!(
            events[1],
            AgentEvent::Commands {
                commands: vec![
                    AgentCommand::new("design", CommandSource::Skill),
                    AgentCommand::new("studio:mockup", CommandSource::Plugin),
                    AgentCommand::new("compact", CommandSource::Unknown),
                ]
            }
        );
    }

    #[test]
    fn permission_and_plan_requests() {
        let mut t = translator();
        let none = HashSet::new();
        let plan = json!({"type":"control_request","request_id":"p1","request":{
            "subtype":"can_use_tool","tool_name":"ExitPlanMode","input":{"plan":"1. Buscar\n2. Cambiar"}}});
        assert_eq!(
            t.translate(&plan, &none),
            vec![AgentEvent::Plan {
                text: "1. Buscar\n2. Cambiar".into(),
                request_id: Some("p1".into()),
            }]
        );
        let bash = json!({"type":"control_request","request_id":"r1","request":{
            "subtype":"can_use_tool","tool_name":"Bash","input":{"command":"pytest -q"}}});
        let events = t.translate(&bash, &none);
        assert!(
            matches!(&events[0], AgentEvent::PermissionRequest { request_id, tool, summary, .. }
            if request_id == "r1" && tool == "Bash" && summary == "pytest -q")
        );
        assert!(
            t.translate(&json!({"type":"control_response","response":{}}), &none)
                .is_empty()
        );
    }

    #[test]
    fn activity_comes_from_results_and_skips_denied_edits() {
        let mut t = translator();
        let assistant = json!({"type":"assistant","message":{"content":[
            {"type":"tool_use","id":"e1","name":"Edit","input":{"file_path":"/wt/citas/servicio.py"}},
            {"type":"tool_use","id":"e2","name":"Write","input":{"file_path":"/wt/.env"}},
            {"type":"tool_use","id":"p1","name":"Write","input":{"file_path":"/home/e/.claude/plans/plan.md"}},
            {"type":"tool_use","id":"b1","name":"Bash","input":{"command":"pytest tests"}},
            {"type":"tool_use","id":"g1","name":"Grep","input":{"pattern":"crear_cita"}},
            {"type":"tool_use","id":"r1","name":"Read","input":{"file_path":"/wt/.env"}}
        ]}});
        // Only live "started" events for the mascot; the activity lines wait for results.
        let started: Vec<(String, String)> = t
            .translate(&assistant, &HashSet::new())
            .into_iter()
            .map(|e| match e {
                AgentEvent::ToolStarted { tool, detail } => (tool, detail),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(started.len(), 6);
        assert_eq!(started[0], ("Edit".into(), "citas/servicio.py".into()));
        assert_eq!(started[3], ("Bash".into(), "pytest tests".into()));
        let results = json!({"type":"user","message":{"content":[
            {"type":"tool_result","tool_use_id":"e1","content":"ok"},
            {"type":"tool_result","tool_use_id":"e2","content":"denied","is_error":true},
            {"type":"tool_result","tool_use_id":"p1","content":"ok"},
            {"type":"tool_result","tool_use_id":"b1","content":"1 failed","is_error":true},
            {"type":"tool_result","tool_use_id":"g1","content":"3 matches"},
            {"type":"tool_result","tool_use_id":"r1","content":"Permission denied","is_error":true}
        ]}});
        let denied = HashSet::from(["e2".to_owned()]);
        assert_eq!(
            t.translate(&results, &denied),
            vec![
                AgentEvent::FileEdited {
                    path: "citas/servicio.py".into()
                },
                AgentEvent::TestsRun {
                    command: "pytest tests".into(),
                    passed: false
                },
                AgentEvent::ToolUsed {
                    tool: "Grep".into(),
                    summary: "crear_cita".into()
                },
            ]
        );
    }

    #[test]
    fn hook_blocks_are_reported_as_blocks() {
        let mut t = translator();
        t.translate(
            &json!({"type":"assistant","message":{"content":[
            {"type":"tool_use","id":"b9","name":"Bash","input":{"command":"cat .env"}}]}}),
            &HashSet::new(),
        );
        let result = json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"b9","is_error":true,
            "content":[{"type":"text","text":"Hedy no permite comandos sobre «.env» (archivo protegido)."}]}]}});
        assert_eq!(
            t.translate(&result, &HashSet::new()),
            vec![AgentEvent::PermissionResolved {
                request_id: "hook-b9".into(),
                decision: Decision::Deny,
                automatic: true,
                reason: Some("Hedy no permite comandos sobre «.env» (archivo protegido).".into()),
            }]
        );
    }

    #[test]
    fn result_and_rate_limits() {
        let mut t = translator();
        let none = HashSet::new();
        let result = json!({"type":"result","subtype":"success","is_error":false,"result":"Hecho",
            "duration_ms":1200,"total_cost_usd":0.04,
            "usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":300,"cache_creation_input_tokens":5}});
        assert_eq!(
            t.translate(&result, &none),
            vec![
                AgentEvent::Usage(UsageSample {
                    input_tokens: 10,
                    output_tokens: 20,
                    cache_read_tokens: 300,
                    cache_write_tokens: 5,
                    cost_usd: Some(0.04),
                }),
                AgentEvent::Finished {
                    ok: true,
                    summary: "Hecho".into(),
                    duration_ms: Some(1200)
                },
            ]
        );
        let limit = json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","utilization":0.91,"resetsAt":1800000000}});
        assert_eq!(
            t.translate(&limit, &none),
            vec![AgentEvent::RateLimit {
                status: "allowed_warning".into(),
                utilization: Some(91.0),
                resets_at: Some(1_800_000_000),
            }]
        );
    }
}
