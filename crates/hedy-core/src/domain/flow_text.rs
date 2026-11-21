//! A flow as text (docs/decisions.md §42): what `hedy flow show` prints,
//! what `hedy flow edit` opens and what a PR reviews. A strict subset of
//! YAML, so any YAML tool reads it too:
//!
//! ```yaml
//! name: With a fix loop
//! goal: Payments without errors
//! steps:
//!   - role: programmer
//!     model: sonnet
//!   - check: tests
//!     command: cargo test
//!     on_fail: 1
//!     max_rounds: 2
//!   - parallel:
//!       - role: reviewer
//!       - role: security
//!   - check: verdict
//!     on_fail: 1
//!   - approval
//! triggers: [manual, task_end]
//! ```
//!
//! Steps are numbered from 1. Each goes on to the next one (the last to the
//! end) unless it says `next:` (a step number or `end`); a check also says
//! where it goes when it fails, `on_fail:`. The start and the end are
//! implicit; positions on the canvas are not kept.

use std::collections::{BTreeMap, BTreeSet};

use super::AccountId;
use super::flow::{Check, DEFAULT_ROUNDS, Edge, Flow, Frame, Node, NodeKind, Seat, Trigger, When};
use crate::tr;

/// Longest text accepted.
pub const MAX_TEXT: usize = 64 * 1024;

/// Why a text is not a flow. Messages are user-facing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextError {
    /// 1-based; 0 when it is about the whole text.
    pub line: usize,
    pub message: String,
}

impl TextError {
    fn at(line: usize, message: String) -> Self {
        Self { line, message }
    }
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            f.write_str(&self.message)
        } else {
            let line = self.line;
            let prefix = tr!("línea {line}", "line {line}");
            write!(f, "{prefix}: {}", self.message)
        }
    }
}

// ---- Printing -------------------------------------------------------------

/// The flow as text. Steps follow the path from the start, the passing way
/// first.
pub fn to_text(flow: &Flow) -> String {
    let order = step_order(flow);
    let number: BTreeMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i + 1))
        .collect();
    let target = |to: &str| -> String {
        match flow.node(to).map(|n| &n.kind) {
            Some(NodeKind::End) | None => "end".into(),
            _ => number[to].to_string(),
        }
    };
    let mut out = String::new();
    out.push_str(&format!("name: {}\n", scalar(&flow.name)));
    let frame = &flow.frame;
    if !frame.goal.is_empty() {
        if frame.goal.contains('\n') {
            out.push_str("goal: |\n");
            for line in frame.goal.lines() {
                match line {
                    "" => out.push('\n'),
                    line => out.push_str(&format!("  {line}\n")),
                }
            }
        } else {
            out.push_str(&format!("goal: {}\n", scalar(&frame.goal)));
        }
    }
    for (key, list) in [
        ("rules", &frame.rules),
        ("files", &frame.files),
        ("notes", &frame.notes),
    ] {
        if !list.is_empty() {
            out.push_str(&format!("{key}:\n"));
            for item in list {
                out.push_str(&format!("  - {}\n", scalar(item)));
            }
        }
    }
    out.push_str("steps:\n");
    for (i, id) in order.iter().enumerate() {
        let node = flow.node(id).expect("ordered from the flow");
        let mut lines: Vec<String> = Vec::new();
        match &node.kind {
            NodeKind::Role(seat) => lines.extend(seat_lines(seat)),
            NodeKind::Approval => lines.push("approval".into()),
            NodeKind::Condition(check) => {
                let (name, command) = match check {
                    Check::Tests { command } => ("tests", command.as_deref()),
                    Check::Rules => ("rules", None),
                    Check::Verdict => ("verdict", None),
                };
                lines.push(format!("check: {name}"));
                if let Some(command) = command {
                    lines.push(format!("command: {}", scalar(command)));
                }
            }
            NodeKind::Parallel { roles } => {
                lines.push("parallel:".into());
                for seat in roles {
                    for (j, line) in seat_lines(seat).into_iter().enumerate() {
                        let lead = if j == 0 { "  - " } else { "    " };
                        lines.push(format!("{lead}{line}"));
                    }
                }
            }
            NodeKind::Start | NodeKind::End => unreachable!("not a step"),
        }
        let forward = match node.kind {
            NodeKind::Condition(_) => When::Pass,
            _ => When::After,
        };
        if let Some(edge) = flow.edge(id, forward) {
            let following = order.get(i + 1).copied();
            let to_end = following.is_none()
                && matches!(flow.node(&edge.to).map(|n| &n.kind), Some(NodeKind::End));
            if Some(edge.to.as_str()) != following && !to_end {
                lines.push(format!("next: {}", target(&edge.to)));
            }
            if let Some(max) = edge.max_rounds {
                lines.push(format!("next_max_rounds: {max}"));
            }
        }
        if let Some(edge) = flow.edge(id, When::Fail) {
            lines.push(format!("on_fail: {}", target(&edge.to)));
            if let Some(max) = edge.max_rounds {
                lines.push(format!("max_rounds: {max}"));
            }
        }
        for (j, line) in lines.iter().enumerate() {
            let lead = if j == 0 { "  - " } else { "    " };
            out.push_str(&format!("{lead}{line}\n"));
        }
    }
    if !flow.triggers.is_empty() {
        let keys: Vec<_> = flow.triggers.iter().map(|t| t.key()).collect();
        out.push_str(&format!("triggers: [{}]\n", keys.join(", ")));
    }
    out
}

fn seat_lines(seat: &Seat) -> Vec<String> {
    let mut lines = vec![format!("role: {}", scalar(&seat.role))];
    if let Some(account) = &seat.account {
        lines.push(format!("account: {}", scalar(account.as_str())));
    }
    if let Some(model) = &seat.model {
        lines.push(format!("model: {}", scalar(model)));
    }
    lines
}

/// The steps (every node but the start and the ends) in the order a run
/// meets them, the passing way first; any left over go last.
fn step_order(flow: &Flow) -> Vec<&str> {
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    let mut todo: Vec<&str> = flow.start().map(|n| n.id.as_str()).into_iter().collect();
    while let Some(id) = todo.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(node) = flow.node(id) else { continue };
        if !matches!(node.kind, NodeKind::Start | NodeKind::End) {
            order.push(id);
        }
        // Pushed in reverse: the passing way is visited first.
        for when in [When::Fail, When::Pass, When::After] {
            if let Some(edge) = flow.edge(id, when) {
                todo.push(edge.to.as_str());
            }
        }
    }
    for node in &flow.nodes {
        let step = !matches!(node.kind, NodeKind::Start | NodeKind::End);
        if step && !seen.contains(node.id.as_str()) {
            order.push(node.id.as_str());
        }
    }
    order
}

/// A plain scalar when it reads back the same, else a double-quoted one.
fn scalar(value: &str) -> String {
    let plain = !value.is_empty()
        && value.trim() == value
        && !value.starts_with([
            '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%',
            '@', '`',
        ])
        && !value.contains(": ")
        && !value.contains(" #")
        && !value.ends_with(':')
        && !value.chars().any(char::is_control)
        && !matches!(
            value.to_lowercase().as_str(),
            "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "~"
        )
        && value.parse::<f64>().is_err();
    if plain {
        value.to_owned()
    } else {
        serde_json::to_string(value).expect("a string serializes")
    }
}

// ---- Reading --------------------------------------------------------------

/// A value of the subset: a scalar, a list or a map (keys in order), each
/// with the line it starts on.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Scalar(String, usize),
    List(Vec<Value>, usize),
    Map(Vec<(String, Value)>, usize),
}

impl Value {
    fn line(&self) -> usize {
        match self {
            Self::Scalar(_, l) | Self::List(_, l) | Self::Map(_, l) => *l,
        }
    }
}

struct Line<'a> {
    number: usize,
    indent: usize,
    text: &'a str,
}

/// The flow a text describes. It is not validated against the role library
/// here: `Flow::problems` does that.
pub fn from_text(text: &str) -> Result<Flow, TextError> {
    if text.len() > MAX_TEXT {
        return Err(TextError::at(
            0,
            tr!("el texto es demasiado largo", "the text is too long"),
        ));
    }
    let root = parse(text)?;
    let Value::Map(entries, _) = root else {
        return Err(TextError::at(
            root.line(),
            tr!(
                "se esperaba «name:», «steps:»…",
                "expected «name:», «steps:»…"
            ),
        ));
    };
    let mut name = None;
    let mut frame = Frame::default();
    let mut steps = None;
    let mut triggers = Vec::new();
    let mut seen = BTreeSet::new();
    for (key, value) in entries {
        if !seen.insert(key.clone()) {
            return Err(repeated(value.line(), &key));
        }
        match key.as_str() {
            "name" => name = Some(text_of(&value)?),
            "goal" => frame.goal = text_of(&value)?,
            "rules" => frame.rules = texts_of(&value)?,
            "files" => frame.files = texts_of(&value)?,
            "notes" => frame.notes = texts_of(&value)?,
            "steps" => steps = Some(value),
            "triggers" => {
                for item in texts_of(&value)? {
                    let trigger = Trigger::from_key(&item).ok_or_else(|| {
                        TextError::at(
                            value.line(),
                            tr!(
                                "disparador desconocido: «{item}» (manual, task_end o before_commit)",
                                "unknown trigger: «{item}» (manual, task_end or before_commit)"
                            ),
                        )
                    })?;
                    if !triggers.contains(&trigger) {
                        triggers.push(trigger);
                    }
                }
            }
            _ => return Err(unknown_key(value.line(), &key)),
        }
    }
    let name = name.ok_or_else(|| TextError::at(0, tr!("falta «name:»", "«name:» is missing")))?;
    let steps =
        steps.ok_or_else(|| TextError::at(0, tr!("falta «steps:»", "«steps:» is missing")))?;
    let Value::List(items, _) = steps else {
        return Err(TextError::at(
            steps.line(),
            tr!(
                "«steps:» debe ser una lista de pasos («- role: …»)",
                "«steps:» must be a list of steps («- role: …»)"
            ),
        ));
    };
    let (nodes, edges) = steps_to_graph(&items)?;
    Ok(Flow {
        name,
        frame,
        nodes,
        edges,
        triggers,
    })
}

/// A step as read, before it becomes nodes and connections.
struct Step {
    line: usize,
    kind: NodeKind,
    next: Option<Target>,
    next_rounds: Option<u32>,
    on_fail: Option<Target>,
    rounds: Option<u32>,
}

#[derive(Clone, Copy)]
enum Target {
    Step(usize),
    End,
}

fn steps_to_graph(items: &[Value]) -> Result<(Vec<Node>, Vec<Edge>), TextError> {
    let steps: Vec<Step> = items.iter().map(read_step).collect::<Result<_, _>>()?;
    let id = |i: usize| format!("s{}", i + 1);
    let mut nodes = vec![Node {
        id: "start".into(),
        kind: NodeKind::Start,
        at: None,
    }];
    let mut edges = Vec::new();
    let first = if steps.is_empty() {
        "end".into()
    } else {
        id(0)
    };
    edges.push(Edge {
        from: "start".into(),
        to: first,
        when: When::After,
        max_rounds: None,
    });
    let resolve = |target: Target, line: usize| -> Result<String, TextError> {
        match target {
            Target::End => Ok("end".into()),
            Target::Step(n) if (1..=steps.len()).contains(&n) => Ok(id(n - 1)),
            Target::Step(n) => {
                let count = steps.len();
                Err(TextError::at(
                    line,
                    tr!(
                        "no hay un paso {n} (hay {count})",
                        "there is no step {n} (there are {count})"
                    ),
                ))
            }
        }
    };
    for (i, step) in steps.iter().enumerate() {
        let is_check = matches!(step.kind, NodeKind::Condition(_));
        if !is_check && (step.on_fail.is_some() || step.rounds.is_some()) {
            return Err(TextError::at(
                step.line,
                tr!(
                    "solo una comprobación («check:») tiene «on_fail:»",
                    "only a check («check:») has «on_fail:»"
                ),
            ));
        }
        nodes.push(Node {
            id: id(i),
            kind: step.kind.clone(),
            at: None,
        });
        let following = if i + 1 < steps.len() {
            Target::Step(i + 2)
        } else {
            Target::End
        };
        edges.push(Edge {
            from: id(i),
            to: resolve(step.next.unwrap_or(following), step.line)?,
            when: if is_check { When::Pass } else { When::After },
            max_rounds: step.next_rounds,
        });
        if is_check {
            let Some(on_fail) = step.on_fail else {
                return Err(TextError::at(
                    step.line,
                    tr!(
                        "una comprobación necesita «on_fail:» (a qué paso vuelve, o end)",
                        "a check needs «on_fail:» (the step it goes back to, or end)"
                    ),
                ));
            };
            let to = resolve(on_fail, step.line)?;
            // Going back (or to itself) is a loop: it carries its rounds.
            let back = matches!(on_fail, Target::Step(n) if n <= i + 1);
            let rounds = match (step.rounds, back) {
                (Some(r), _) => Some(r),
                (None, true) => Some(DEFAULT_ROUNDS),
                (None, false) => None,
            };
            edges.push(Edge {
                from: id(i),
                to,
                when: When::Fail,
                max_rounds: rounds,
            });
        }
    }
    nodes.push(Node {
        id: "end".into(),
        kind: NodeKind::End,
        at: None,
    });
    Ok((nodes, edges))
}

fn read_step(item: &Value) -> Result<Step, TextError> {
    let line = item.line();
    let mut step = Step {
        line,
        kind: NodeKind::Approval,
        next: None,
        next_rounds: None,
        on_fail: None,
        rounds: None,
    };
    let entries = match item {
        Value::Scalar(word, _) if word == "approval" => return Ok(step),
        Value::Map(entries, _) => entries,
        _ => return Err(not_a_step(line)),
    };
    let mut kind = None;
    let mut seat: Option<Seat> = None;
    let mut command = None;
    let mut seen = BTreeSet::new();
    for (key, value) in entries {
        if !seen.insert(key.as_str()) {
            return Err(repeated(value.line(), key));
        }
        match key.as_str() {
            "role" => seat = Some(Seat::new(&text_of(value)?)),
            "account" | "model" => {
                // Placed on the role read before it.
                let Some(seat) = seat.as_mut() else {
                    return Err(not_a_step(value.line()));
                };
                let text = text_of(value)?;
                if key == "account" {
                    seat.account = Some(
                        AccountId::new(&text)
                            .map_err(|e| TextError::at(value.line(), e.to_string()))?,
                    );
                } else {
                    seat.model = Some(text);
                }
            }
            "approval" => kind = Some(NodeKind::Approval),
            "check" => {
                let check = match text_of(value)?.as_str() {
                    "tests" => Check::Tests { command: None },
                    "rules" => Check::Rules,
                    "verdict" => Check::Verdict,
                    other => {
                        return Err(TextError::at(
                            value.line(),
                            tr!(
                                "comprobación desconocida: «{other}» (tests, rules o verdict)",
                                "unknown check: «{other}» (tests, rules or verdict)"
                            ),
                        ));
                    }
                };
                kind = Some(NodeKind::Condition(check));
            }
            "command" => command = Some(text_of(value)?),
            "parallel" => {
                let Value::List(roles, _) = value else {
                    return Err(not_a_step(value.line()));
                };
                let roles = roles.iter().map(read_seat).collect::<Result<Vec<_>, _>>()?;
                kind = Some(NodeKind::Parallel { roles });
            }
            "next" => step.next = Some(target_of(value)?),
            "next_max_rounds" => step.next_rounds = Some(number_of(value)?),
            "on_fail" => step.on_fail = Some(target_of(value)?),
            "max_rounds" => step.rounds = Some(number_of(value)?),
            _ => return Err(unknown_key(value.line(), key)),
        }
    }
    step.kind = match (kind, seat) {
        (None, Some(seat)) => NodeKind::Role(seat),
        (Some(kind), None) => kind,
        _ => return Err(not_a_step(line)),
    };
    if let Some(command) = command {
        let NodeKind::Condition(Check::Tests { command: slot }) = &mut step.kind else {
            return Err(TextError::at(
                line,
                tr!(
                    "«command:» solo va en «check: tests»",
                    "«command:» only goes with «check: tests»"
                ),
            ));
        };
        *slot = Some(command);
    }
    Ok(step)
}

fn read_seat(item: &Value) -> Result<Seat, TextError> {
    let Value::Map(entries, line) = item else {
        return Err(not_a_step(item.line()));
    };
    let mut seat: Option<Seat> = None;
    for (key, value) in entries {
        let text = text_of(value)?;
        match (key.as_str(), seat.as_mut()) {
            ("role", None) => seat = Some(Seat::new(&text)),
            ("account", Some(seat)) if seat.account.is_none() => {
                seat.account = Some(
                    AccountId::new(&text)
                        .map_err(|e| TextError::at(value.line(), e.to_string()))?,
                );
            }
            ("model", Some(seat)) if seat.model.is_none() => seat.model = Some(text),
            _ => return Err(unknown_key(value.line(), key)),
        }
    }
    seat.ok_or_else(|| not_a_step(*line))
}

fn text_of(value: &Value) -> Result<String, TextError> {
    match value {
        Value::Scalar(text, _) => Ok(text.clone()),
        _ => Err(TextError::at(
            value.line(),
            tr!("se esperaba un texto", "expected a text"),
        )),
    }
}

fn texts_of(value: &Value) -> Result<Vec<String>, TextError> {
    match value {
        Value::List(items, _) => items.iter().map(text_of).collect(),
        _ => Err(TextError::at(
            value.line(),
            tr!("se esperaba una lista", "expected a list"),
        )),
    }
}

fn number_of(value: &Value) -> Result<u32, TextError> {
    text_of(value)?.parse().map_err(|_| {
        TextError::at(
            value.line(),
            tr!("se esperaba un número", "expected a number"),
        )
    })
}

fn target_of(value: &Value) -> Result<Target, TextError> {
    let text = text_of(value)?;
    if text == "end" {
        return Ok(Target::End);
    }
    text.parse().map(Target::Step).map_err(|_| {
        TextError::at(
            value.line(),
            tr!(
                "se esperaba un número de paso o end",
                "expected a step number or end"
            ),
        )
    })
}

fn not_a_step(line: usize) -> TextError {
    TextError::at(
        line,
        tr!(
            "un paso es «role:», «check:», «parallel:» o «approval»",
            "a step is «role:», «check:», «parallel:» or «approval»"
        ),
    )
}

fn unknown_key(line: usize, key: &str) -> TextError {
    TextError::at(
        line,
        tr!("clave desconocida: «{key}»", "unknown key: «{key}»"),
    )
}

fn repeated(line: usize, key: &str) -> TextError {
    TextError::at(line, tr!("«{key}» está repetido", "«{key}» is repeated"))
}

/// The subset of YAML: block maps and lists by indentation (spaces), plain
/// or double-quoted scalars, `[a, b]` lists of plain scalars, `|` blocks and
/// `#` comments.
fn parse(text: &str) -> Result<Value, TextError> {
    let mut lines = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let number = i + 1;
        let trimmed = raw.trim_end();
        let indent = trimmed.len() - trimmed.trim_start().len();
        if trimmed[..indent].contains('\t') {
            return Err(TextError::at(
                number,
                tr!(
                    "usa espacios para sangrar, no tabuladores",
                    "indent with spaces, not tabs"
                ),
            ));
        }
        lines.push(Line {
            number,
            indent,
            text: trimmed.trim_start(),
        });
    }
    let mut at = 0;
    skip_blank(&lines, &mut at);
    if at == lines.len() {
        return Err(TextError::at(
            0,
            tr!("el texto está vacío", "the text is empty"),
        ));
    }
    let indent = lines[at].indent;
    let value = block(&lines, &mut at, indent)?;
    skip_blank(&lines, &mut at);
    if let Some(line) = lines.get(at) {
        return Err(bad_indent(line.number));
    }
    Ok(value)
}

fn skip_blank(lines: &[Line], at: &mut usize) {
    while lines
        .get(*at)
        .is_some_and(|l| l.text.is_empty() || l.text.starts_with('#'))
    {
        *at += 1;
    }
}

fn bad_indent(line: usize) -> TextError {
    TextError::at(line, tr!("sangría inesperada", "unexpected indentation"))
}

/// A list or a map whose lines start at column `indent`.
fn block(lines: &[Line], at: &mut usize, indent: usize) -> Result<Value, TextError> {
    let first = lines[*at].number;
    let is_list = item_rest(lines[*at].text).is_some();
    let mut items = Vec::new();
    let mut entries: Vec<(String, Value)> = Vec::new();
    loop {
        skip_blank(lines, at);
        let Some(line) = lines.get(*at) else { break };
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(bad_indent(line.number));
        }
        match (is_list, item_rest(line.text)) {
            (true, Some(rest)) => {
                *at += 1;
                // Where the item's own text starts: its map keys line up there.
                let column = indent + (line.text.len() - rest.len());
                items.push(item(lines, at, rest, column, line.number)?);
            }
            (false, None) => {
                *at += 1;
                let (key, value) = entry(lines, at, line.text, indent, line.number)?;
                entries.push((key, value));
            }
            _ => break,
        }
    }
    Ok(if is_list {
        Value::List(items, first)
    } else {
        Value::Map(entries, first)
    })
}

/// The text after `- `, for a list item.
fn item_rest(text: &str) -> Option<&str> {
    if text == "-" {
        Some("")
    } else {
        text.strip_prefix("- ").map(str::trim_start)
    }
}

fn item(
    lines: &[Line],
    at: &mut usize,
    rest: &str,
    column: usize,
    number: usize,
) -> Result<Value, TextError> {
    if rest.is_empty() {
        return nested(lines, at, column, number);
    }
    if split_key(rest).is_some() {
        // A map starting on the item's line; the rest of its keys line up.
        let (key, value) = entry(lines, at, rest, column, number)?;
        let mut entries = vec![(key, value)];
        let mut probe = *at;
        skip_blank(lines, &mut probe);
        let more = lines
            .get(probe)
            .is_some_and(|l| l.indent == column && item_rest(l.text).is_none());
        if more {
            *at = probe;
            if let Value::Map(rest, _) = block(lines, at, column)? {
                entries.extend(rest);
            }
        }
        return Ok(Value::Map(entries, number));
    }
    Ok(Value::Scalar(plain(rest, number)?, number))
}

/// The block under a line that ends in `:` or `-` (deeper, or a list at the
/// same column as a map key).
fn nested(
    lines: &[Line],
    at: &mut usize,
    column: usize,
    number: usize,
) -> Result<Value, TextError> {
    let mut probe = *at;
    skip_blank(lines, &mut probe);
    match lines.get(probe) {
        Some(line)
            if line.indent > column
                || (line.indent == column && item_rest(line.text).is_some()) =>
        {
            *at = probe;
            block(lines, at, line.indent)
        }
        _ => Err(TextError::at(
            number,
            tr!("falta el valor", "the value is missing"),
        )),
    }
}

/// `key: value`, or `key:` with its block below.
fn split_key(text: &str) -> Option<(&str, &str)> {
    let (key, value) = match text.split_once(": ") {
        Some((key, value)) => (key, value),
        None => (text.strip_suffix(':')?, ""),
    };
    let ok = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    ok.then_some((key, value.trim()))
}

fn entry(
    lines: &[Line],
    at: &mut usize,
    text: &str,
    column: usize,
    number: usize,
) -> Result<(String, Value), TextError> {
    let Some((key, value)) = split_key(text) else {
        return Err(TextError::at(
            number,
            tr!("se esperaba «clave: valor»", "expected «key: value»"),
        ));
    };
    let value = if value.is_empty() {
        // `key:` then a list at the same column is a list too.
        nested(lines, at, column, number)?
    } else if value == "|" {
        Value::Scalar(literal(lines, at, column), number)
    } else if let Some(inner) = value.strip_prefix('[') {
        let inner = inner
            .strip_suffix(']')
            .ok_or_else(|| TextError::at(number, tr!("falta «]»", "«]» is missing")))?;
        let items = inner
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| plain(s, number).map(|t| Value::Scalar(t, number)))
            .collect::<Result<_, _>>()?;
        Value::List(items, number)
    } else {
        Value::Scalar(plain(value, number)?, number)
    };
    Ok((key.to_owned(), value))
}

/// A `|` block: the lines deeper than `column`, as written.
fn literal(lines: &[Line], at: &mut usize, column: usize) -> String {
    let mut body: Vec<(usize, &str)> = Vec::new();
    while let Some(line) = lines.get(*at) {
        if !line.text.is_empty() && line.indent <= column {
            break;
        }
        body.push((line.indent, line.text));
        *at += 1;
    }
    while body.last().is_some_and(|(_, t)| t.is_empty()) {
        body.pop();
    }
    let base = body
        .iter()
        .filter(|(_, t)| !t.is_empty())
        .map(|(i, _)| *i)
        .min()
        .unwrap_or(0);
    body.iter()
        .map(|(indent, text)| {
            if text.is_empty() {
                String::new()
            } else {
                format!("{}{text}", " ".repeat(indent - base))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A scalar: double-quoted (JSON escapes) or plain, with a trailing ` #`
/// comment dropped.
fn plain(text: &str, number: usize) -> Result<String, TextError> {
    if text.starts_with('"') {
        let mut de = serde_json::Deserializer::from_str(text).into_iter::<String>();
        let value = de
            .next()
            .and_then(Result::ok)
            .ok_or_else(|| TextError::at(number, tr!("comillas sin cerrar", "unclosed quotes")))?;
        let rest = text[de.byte_offset()..].trim();
        if !(rest.is_empty() || rest.starts_with('#')) {
            return Err(TextError::at(
                number,
                tr!("texto después de las comillas", "text after the quotes"),
            ));
        }
        return Ok(value);
    }
    let text = match text.find(" #") {
        Some(i) => &text[..i],
        None => text,
    };
    if text.starts_with(['\'', '{', '&', '*', '!', '|', '>']) {
        return Err(TextError::at(
            number,
            tr!(
                "usa comillas dobles para este texto",
                "use double quotes for this text"
            ),
        ));
    }
    Ok(text.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::PhasePlan;
    use crate::domain::flow::{Template, builtin_role};

    const SAMPLE: &str = r#"
# A flow for the shop.
name: Pasarela de pagos
goal: |
  Cobrar con tarjeta.

  Sin guardar datos de la tarjeta.
rules:
  - "No tocar la API pública: nunca"
steps:
  - role: architect
    model: opus
  - approval
  - role: programmer
    account: claude-personal
  - check: tests
    command: cargo test -p pagos   # the fast ones
    on_fail: 3
  - parallel:
      - role: reviewer
        model: fable
      - role: security
  - check: verdict
    on_fail: 3
    max_rounds: 3
triggers: [manual, task_end]
"#;

    #[test]
    fn reads_a_flow() {
        let flow = from_text(SAMPLE).unwrap();
        assert_eq!(flow.name, "Pasarela de pagos");
        assert_eq!(
            flow.frame.goal,
            "Cobrar con tarjeta.\n\nSin guardar datos de la tarjeta."
        );
        assert_eq!(flow.frame.rules, vec!["No tocar la API pública: nunca"]);
        assert_eq!(flow.triggers, vec![Trigger::Manual, Trigger::TaskEnd]);
        assert_eq!(flow.problems(&builtin_role), vec![]);
        let tests = flow.node("s4").unwrap();
        assert_eq!(
            tests.kind,
            NodeKind::Condition(Check::Tests {
                command: Some("cargo test -p pagos".into())
            })
        );
        let back = flow.edge("s4", When::Fail).unwrap();
        assert_eq!((back.to.as_str(), back.max_rounds), ("s3", Some(2)));
        assert_eq!(flow.edge("s6", When::Fail).unwrap().max_rounds, Some(3));
        assert_eq!(flow.edge("s6", When::Pass).unwrap().to, "end");
        let seats: Vec<_> = flow.seats().map(|s| s.role.as_str()).collect();
        assert_eq!(
            seats,
            vec!["architect", "programmer", "reviewer", "security"]
        );
        assert_eq!(
            flow.node("s3").unwrap().kind,
            NodeKind::Role(Seat {
                role: "programmer".into(),
                account: Some(AccountId::new("claude-personal").unwrap()),
                model: None
            })
        );
    }

    #[test]
    fn text_round_trips() {
        let flow = from_text(SAMPLE).unwrap();
        let text = to_text(&flow);
        let again = from_text(&text).unwrap();
        assert_eq!(again, flow, "{text}");
        assert_eq!(to_text(&again), text);
        for template in Template::ALL {
            let flow = template.flow(&PhasePlan::new());
            let text = to_text(&flow);
            let back = from_text(&text).unwrap();
            assert_eq!(back.problems(&builtin_role), vec![], "{text}");
            assert_eq!(to_text(&back), text);
        }
    }

    #[test]
    fn the_fix_loop_reads_like_the_plan() {
        let text = to_text(&Template::FixLoop.flow(&PhasePlan::new()));
        assert!(
            text.contains(
                "steps:\n  - role: programmer\n  - check: tests\n    on_fail: 1\n    max_rounds: 2\n  - role: reviewer\n"
            ),
            "{text}"
        );
        assert!(text.ends_with("triggers: [manual]\n"), "{text}");
    }

    #[test]
    fn odd_texts_are_quoted_and_read_back() {
        for value in [
            "- x",
            "a: b",
            "true",
            "12",
            " lead",
            "say #no",
            "\"q\"",
            "tab\there",
        ] {
            let mut flow = Template::Classic.flow(&PhasePlan::new());
            flow.name = value.into();
            flow.frame.rules = vec![value.into()];
            let back = from_text(&to_text(&flow)).unwrap();
            assert_eq!(back.name, value);
            assert_eq!(back.frame.rules, vec![value.to_owned()]);
        }
    }

    #[test]
    fn mistakes_say_where() {
        let error = |text: &str| from_text(text).unwrap_err();
        assert_eq!(error("name: x\nsteps:\n  - role: a\n  - jump: 3\n").line, 4);
        assert_eq!(error("name: x\nsteps:\n  - check: tests\n").line, 3);
        assert_eq!(
            error("name: x\nsteps:\n  - check: tests\n    on_fail: 9\n").line,
            3
        );
        assert_eq!(
            error("name: x\nsteps:\n  - role: a\n    on_fail: 1\n").line,
            3
        );
        assert_eq!(error("name: x\n\tsteps: []\n").line, 2);
        assert_eq!(
            error("name: x\nsteps:\n  - role: a\n   - role: b\n").line,
            4
        );
        assert_eq!(error("steps: []\n").line, 0);
        assert_eq!(error("name: x\nname: y\nsteps: []\n").line, 2);
        assert_eq!(error("name: x\nsteps: []\ntriggers: [cron]\n").line, 3);
        assert_eq!(
            error("name: x\nsteps:\n  - role: a\n    account: \"Mala Cuenta!\"\n").line,
            4
        );
        assert_eq!(error("").line, 0);
    }

    #[test]
    fn an_empty_list_of_steps_goes_straight_to_the_end() {
        let flow = from_text("name: x\nsteps: []\n").unwrap();
        assert_eq!(flow.edge("start", When::After).unwrap().to, "end");
    }
}
