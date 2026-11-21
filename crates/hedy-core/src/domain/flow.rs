//! The orchestrator's flows (docs/decisions.md §42): a team of agents working
//! on one task. A flow is a graph: a start, roles (agents with their own
//! instructions, account and model), approvals, conditions that pick a path,
//! groups of read-only roles at once, and an end. Every cycle carries its
//! maximum number of rounds, and only one role writes at a time.
//!
//! Pure: the engine in `application::flows` runs it; the text form lives in
//! `flow_text`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use super::{AccountId, Phase, PhasePlan};
use crate::tr;

/// Rounds a loop runs when nobody says otherwise.
pub const DEFAULT_ROUNDS: u32 = 2;
/// Most rounds a loop may carry: every round spends tokens again.
pub const MAX_ROUNDS: u32 = 10;
/// Most nodes in a flow.
pub const MAX_NODES: usize = 40;
/// Longest flow or role name.
pub const MAX_NAME: usize = 60;
/// Longest role instructions.
pub const MAX_INSTRUCTIONS: usize = 8_000;

/// Who works on a role's step: `account: None` is the project's account,
/// `model: None` lets the provider choose. Chosen when the role is placed in
/// a flow, never in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Seat {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl Seat {
    pub fn new(role: &str) -> Self {
        Self {
            role: role.to_owned(),
            account: None,
            model: None,
        }
    }
}

/// What picks the path after a step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "check", rename_all = "camelCase")]
pub enum Check {
    /// The project's tests pass. `command: None`: the one Hedy finds for the
    /// project (`domain::briefing::test_command`).
    Tests {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<String>,
    },
    /// The change follows the written rules (`hedy review`, §37).
    Rules,
    /// The reviewers before it approve (their structured verdict).
    Verdict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum NodeKind {
    Start,
    Role(Seat),
    /// Waits until the user approves.
    Approval,
    Condition(Check),
    /// Several read-only roles on the same state; waits for all of them.
    Parallel {
        roles: Vec<Seat>,
    },
    /// Leaves the changes for review.
    End,
}

/// Where a node sits on the canvas (the engine ignores it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    #[serde(flatten)]
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Point>,
}

/// When a connection is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum When {
    /// After a step that is not a condition.
    After,
    /// The condition passed (tests pass, rules kept, approved).
    Pass,
    /// The condition failed (tests fail, a rule broken, changes requested).
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub when: When,
    /// How many times a run may take it: required on any connection that
    /// closes a cycle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rounds: Option<u32>,
}

/// What starts a flow besides the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Trigger {
    /// By hand: the composer, a template, `hedy flow run`.
    Manual,
    /// When a task of the project ends.
    TaskEnd,
    /// Before a commit (the pre-commit hook).
    BeforeCommit,
}

impl Trigger {
    pub const ALL: [Trigger; 3] = [Self::Manual, Self::TaskEnd, Self::BeforeCommit];

    /// The key used in the text form.
    pub fn key(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::TaskEnd => "task_end",
            Self::BeforeCommit => "before_commit",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    pub fn label(self) -> String {
        match self {
            Self::Manual => tr!("a mano", "by hand"),
            Self::TaskEnd => tr!("al terminar una tarea", "when a task ends"),
            Self::BeforeCommit => tr!("antes de un commit", "before a commit"),
        }
    }
}

/// What every role of the flow shares: the goal and the rules of the work,
/// files to look at and approved notes to read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub goal: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// Topics of approved notes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Flow {
    pub name: String,
    #[serde(default)]
    pub frame: Frame,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub triggers: Vec<Trigger>,
}

/// What a role hands over.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Delivers {
    #[default]
    Text,
    /// Approve or request changes, with reasons: only such a role can feed
    /// a verdict condition.
    Verdict,
}

/// What a role may do besides reading.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Abilities {
    pub write: bool,
    /// Run commands (tests, builds).
    pub run: bool,
}

/// The blackboard sections a role reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "sees", content = "sections", rename_all = "camelCase")]
pub enum Sees {
    #[default]
    All,
    Sections(Vec<String>),
}

/// Skills and notes loaded with a role.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Carries {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// A role in the library: defined once, placed in any flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleDef {
    /// Lowercase with dashes; what a flow names.
    pub id: String,
    pub name: String,
    /// One line of what it does.
    #[serde(default)]
    pub summary: String,
    pub instructions: String,
    #[serde(default)]
    pub carries: Carries,
    #[serde(default)]
    pub delivers: Delivers,
    #[serde(default)]
    pub can: Abilities,
    #[serde(default)]
    pub sees: Sees,
    /// Proposes a plan and waits for approval before changing anything.
    #[serde(default)]
    pub plan_first: bool,
}

impl RoleDef {
    /// The phase its agent runs as: it decides the tools an adapter gives it
    /// (a read-only phase gets no write tools).
    pub fn phase(&self) -> Phase {
        match (self.can.write, self.plan_first) {
            (false, _) => Phase::Review,
            (true, true) => Phase::Plan,
            (true, false) => Phase::Implement,
        }
    }

    pub fn read_only(&self) -> bool {
        !self.can.write
    }

    /// The board section its result goes to.
    pub fn section(&self) -> String {
        match self.id.as_str() {
            "architect" => "plan",
            "programmer" => "changes",
            "tester" => "tests",
            "reviewer" => "review",
            other => other,
        }
        .to_owned()
    }

    /// Whether it reads a board section: always its own (what it did
    /// before, when it is sent back) and the user's feedback.
    pub fn sees_section(&self, section: &str) -> bool {
        section == FEEDBACK
            || section == self.section()
            || match &self.sees {
                Sees::All => true,
                Sees::Sections(list) => list.iter().any(|s| s == section),
            }
    }
}

/// The roles that ship with Hedy, in the current language.
pub fn builtin_roles() -> Vec<RoleDef> {
    let role = |id: &str, name: String, summary: String, instructions: String| RoleDef {
        id: id.to_owned(),
        name,
        summary,
        instructions,
        carries: Carries::default(),
        delivers: Delivers::Text,
        can: Abilities::default(),
        sees: Sees::All,
        plan_first: false,
    };
    let mut architect = role(
        "architect",
        tr!("Arquitecto", "Architect"),
        tr!(
            "Lee el proyecto y escribe el plan",
            "Reads the project and writes the plan"
        ),
        tr!(
            "Eres el arquitecto. Lee el pedido y el proyecto y escribe un plan breve y numerado: qué cambiar, en qué archivos y cómo comprobarlo. No cambies archivos.",
            "You are the architect. Read the request and the project and write a short numbered plan: what to change, in which files and how to check it. Do not change files."
        ),
    );
    architect.sees = Sees::Sections(vec!["frame".into()]);
    let mut programmer = role(
        "programmer",
        tr!("Programador", "Programmer"),
        tr!("Implementa el plan", "Implements the plan"),
        tr!(
            "Eres el programador. Implementa el pedido siguiendo el plan de la pizarra. Si la pizarra trae fallos de tests o cambios pedidos por la revisión, arréglalos primero.",
            "You are the programmer. Implement the request following the plan on the blackboard. If the blackboard has failing tests or changes requested by the review, fix those first."
        ),
    );
    programmer.can = Abilities {
        write: true,
        run: true,
    };
    let mut tester = role(
        "tester",
        tr!("Tester", "Tester"),
        tr!("Escribe y corre los tests", "Writes and runs the tests"),
        tr!(
            "Eres el tester. Escribe o actualiza los tests de los cambios de esta tarea y córrelos. No cambies el código de producción salvo para arreglar un error evidente.",
            "You are the tester. Write or update the tests for this task's changes and run them. Do not change production code except to fix an obvious bug."
        ),
    );
    tester.can = Abilities {
        write: true,
        run: true,
    };
    let mut reviewer = role(
        "reviewer",
        tr!("Revisor", "Reviewer"),
        tr!(
            "Revisa los cambios y da un veredicto",
            "Reviews the changes and gives a verdict"
        ),
        tr!(
            "Eres el revisor. Revisa los cambios de esta tarea (usa git diff): errores, casos sin cubrir, tests. Solo señalas; no cambias archivos.",
            "You are the reviewer. Review this task's changes (use git diff): bugs, uncovered cases, tests. You only point things out; you do not change files."
        ),
    );
    reviewer.delivers = Delivers::Verdict;
    let mut security = role(
        "security",
        tr!("Seguridad", "Security"),
        tr!(
            "Busca problemas de seguridad y privacidad",
            "Looks for security and privacy problems"
        ),
        tr!(
            "Eres el revisor de seguridad. Revisa los cambios de esta tarea (usa git diff) buscando problemas de seguridad y privacidad: secretos, entradas sin validar, permisos, datos personales. Solo señalas; no cambias archivos.",
            "You are the security reviewer. Review this task's changes (use git diff) looking for security and privacy problems: secrets, unvalidated input, permissions, personal data. You only point things out; you do not change files."
        ),
    );
    security.delivers = Delivers::Verdict;
    vec![architect, programmer, tester, reviewer, security]
}

/// A built-in role by id.
pub fn builtin_role(id: &str) -> Option<RoleDef> {
    builtin_roles().into_iter().find(|r| r.id == id)
}

/// Hedy's flow templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    /// Today's pipeline: plan, your OK, implement, tests and review.
    Classic,
    /// Implement, then tests and a review that send it back (max 2 rounds).
    FixLoop,
    /// Implement, then security and style reviews at once, read-only.
    DoubleReview,
}

impl Template {
    pub const ALL: [Template; 3] = [Self::Classic, Self::FixLoop, Self::DoubleReview];

    pub fn key(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::FixLoop => "fix-loop",
            Self::DoubleReview => "double-review",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    pub fn name(self) -> String {
        match self {
            Self::Classic => tr!("Pipeline clásico", "Classic pipeline"),
            Self::FixLoop => tr!("Con bucle de corrección", "With a fix loop"),
            Self::DoubleReview => tr!("Doble revisión", "Double review"),
        }
    }

    pub fn summary(self) -> String {
        match self {
            Self::Classic => tr!(
                "Plan, tu OK, implementación, tests y revisión: lo de siempre",
                "Plan, your OK, implementation, tests and review: as always"
            ),
            Self::FixLoop => tr!(
                "Si los tests fallan o la revisión pide cambios, vuelve al programador (máx. 2)",
                "If tests fail or the review asks for changes, back to the programmer (max 2)"
            ),
            Self::DoubleReview => tr!(
                "Revisión y seguridad a la vez, solo leen",
                "Review and security at once, read-only"
            ),
        }
    }

    /// The template as a flow. The classic one takes the project's phases
    /// (who plans, implements, tests and reviews).
    pub fn flow(self, phases: &PhasePlan) -> Flow {
        let seat = |role: &str, phase: Phase| {
            let mut seat = Seat::new(role);
            if let Some(a) = phases.get(&phase) {
                seat.account = Some(a.account.clone());
                seat.model = a.model.clone();
            }
            seat
        };
        let mut b = Builder::default();
        match self {
            Self::Classic => {
                b.node(NodeKind::Role(seat("architect", Phase::Plan)));
                b.node(NodeKind::Approval);
                b.node(NodeKind::Role(seat("programmer", Phase::Implement)));
                if phases.contains_key(&Phase::Tests) {
                    b.node(NodeKind::Role(seat("tester", Phase::Tests)));
                }
                if phases.contains_key(&Phase::Review) {
                    b.node(NodeKind::Role(seat("reviewer", Phase::Review)));
                }
            }
            Self::FixLoop => {
                let programmer = b.node(NodeKind::Role(seat("programmer", Phase::Implement)));
                b.node(NodeKind::Condition(Check::Tests { command: None }));
                b.back_to(&programmer);
                b.node(NodeKind::Role(seat("reviewer", Phase::Review)));
                b.node(NodeKind::Condition(Check::Verdict));
                b.back_to(&programmer);
            }
            Self::DoubleReview => {
                let programmer = b.node(NodeKind::Role(seat("programmer", Phase::Implement)));
                b.node(NodeKind::Parallel {
                    roles: vec![seat("reviewer", Phase::Review), Seat::new("security")],
                });
                b.node(NodeKind::Condition(Check::Verdict));
                b.back_to(&programmer);
            }
        }
        b.finish(self.name())
    }
}

/// Builds a straight flow, step by step, with loops back.
#[derive(Default)]
struct Builder {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// Fail connections still waiting for their target.
    loops: Vec<(String, String)>,
}

impl Builder {
    fn node(&mut self, kind: NodeKind) -> String {
        if self.nodes.is_empty() && kind != NodeKind::Start {
            self.node(NodeKind::Start);
        }
        let id = format!("n{}", self.nodes.len() + 1);
        if let Some(last) = self.nodes.last() {
            let when = match last.kind {
                NodeKind::Condition(_) => When::Pass,
                _ => When::After,
            };
            self.edges.push(Edge {
                from: last.id.clone(),
                to: id.clone(),
                when,
                max_rounds: None,
            });
        }
        self.nodes.push(Node {
            id: id.clone(),
            kind,
            at: None,
        });
        id
    }

    /// The last node (a condition) goes back to `to` when it fails.
    fn back_to(&mut self, to: &str) {
        let from = self.nodes.last().expect("a condition first").id.clone();
        self.loops.push((from, to.to_owned()));
    }

    fn finish(mut self, name: String) -> Flow {
        self.node(NodeKind::End);
        for (from, to) in std::mem::take(&mut self.loops) {
            self.edges.push(Edge {
                from,
                to,
                when: When::Fail,
                max_rounds: Some(DEFAULT_ROUNDS),
            });
        }
        Flow {
            name,
            frame: Frame::default(),
            nodes: self.nodes,
            edges: self.edges,
            triggers: vec![Trigger::Manual],
        }
    }
}

/// Something that makes a flow unrunnable. Messages are user-facing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowProblem {
    BadName,
    TooManyNodes,
    Starts(usize),
    NoEnd,
    DuplicateNode(String),
    BadNodeId(String),
    UnknownNode(String),
    /// A node without the connections its kind needs, or with extra ones.
    Connections(String),
    Unreachable(String),
    DeadEnd(String),
    UnknownRole(String),
    BadSeat(String),
    /// A parallel group needs two roles or more.
    SmallParallel(String),
    /// Only read-only roles work at once.
    ParallelWriter(String),
    /// A verdict condition must follow roles that deliver one.
    NoVerdict(String),
    EmptyCommand(String),
    /// A cycle with no connection carrying its rounds.
    UnboundedLoop(String),
    BadRounds(String),
}

impl FlowProblem {
    pub fn message(&self) -> String {
        match self {
            Self::BadName => tr!(
                "el flujo necesita un nombre de una línea (hasta {MAX_NAME} caracteres)",
                "the flow needs a one-line name (up to {MAX_NAME} characters)"
            ),
            Self::TooManyNodes => tr!(
                "el flujo tiene más de {MAX_NODES} pasos",
                "the flow has more than {MAX_NODES} steps"
            ),
            Self::Starts(n) => tr!(
                "el flujo necesita exactamente un inicio (tiene {n})",
                "the flow needs exactly one start (it has {n})"
            ),
            Self::NoEnd => tr!("el flujo no tiene final", "the flow has no end"),
            Self::DuplicateNode(id) => {
                tr!("el paso «{id}» está repetido", "step «{id}» is repeated")
            }
            Self::BadNodeId(id) => tr!(
                "«{id}» no sirve como id de un paso (letras, números, - o _)",
                "«{id}» is not a valid step id (letters, digits, - or _)"
            ),
            Self::UnknownNode(id) => tr!(
                "una conexión nombra un paso que no existe: «{id}»",
                "a connection names a step that does not exist: «{id}»"
            ),
            Self::Connections(id) => tr!(
                "el paso «{id}» no tiene las conexiones que necesita (una condición: si pasa y si falla; los demás: una sola salida; el final: ninguna)",
                "step «{id}» does not have the connections it needs (a condition: if it passes and if it fails; the rest: a single way out; the end: none)"
            ),
            Self::Unreachable(id) => tr!(
                "no se llega al paso «{id}» desde el inicio",
                "step «{id}» cannot be reached from the start"
            ),
            Self::DeadEnd(id) => tr!(
                "desde el paso «{id}» no se llega al final",
                "the end cannot be reached from step «{id}»"
            ),
            Self::UnknownRole(role) => tr!(
                "no hay un rol «{role}» en la biblioteca",
                "there is no role «{role}» in the library"
            ),
            Self::BadSeat(id) => tr!(
                "el paso «{id}» tiene un modelo no válido",
                "step «{id}» has an invalid model"
            ),
            Self::SmallParallel(id) => tr!(
                "el grupo en paralelo «{id}» necesita al menos dos roles",
                "the parallel group «{id}» needs at least two roles"
            ),
            Self::ParallelWriter(role) => tr!(
                "«{role}» escribe: en paralelo solo trabajan roles que leen (un solo escritor a la vez)",
                "«{role}» writes: only read-only roles work in parallel (one writer at a time)"
            ),
            Self::NoVerdict(id) => tr!(
                "la condición «{id}» decide por un veredicto, pero el paso anterior no entrega uno",
                "condition «{id}» decides by a verdict, but the step before it does not deliver one"
            ),
            Self::EmptyCommand(id) => tr!(
                "la condición «{id}» tiene un comando vacío o de varias líneas",
                "condition «{id}» has an empty or multi-line command"
            ),
            Self::UnboundedLoop(id) => tr!(
                "el bucle que pasa por «{id}» no tiene un máximo de vueltas",
                "the loop through «{id}» has no maximum of rounds"
            ),
            Self::BadRounds(id) => tr!(
                "una conexión de «{id}» tiene un máximo de vueltas fuera de 1–{MAX_ROUNDS}",
                "a connection from «{id}» has a maximum of rounds outside 1–{MAX_ROUNDS}"
            ),
        }
    }
}

/// A one-line name of reasonable length.
pub fn valid_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty() && name.chars().count() <= MAX_NAME && !name.contains(['\n', '\r'])
}

/// Lowercase letters, digits and dashes: a role's id.
pub fn valid_role_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_NAME
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
        // Its board section would be the user's feedback or the frame.
        && id != FEEDBACK
        && id != FRAME
}

fn valid_node_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn valid_model(model: &Option<String>) -> bool {
    model.as_deref().is_none_or(|m| {
        !m.trim().is_empty()
            && m.len() <= 100
            && !m.starts_with('-')
            && m.chars().all(|c| !c.is_whitespace() && !c.is_control())
    })
}

impl Flow {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn start(&self) -> Option<&Node> {
        self.nodes.iter().find(|n| n.kind == NodeKind::Start)
    }

    /// The connection taken from `from` when `when`.
    pub fn edge(&self, from: &str, when: When) -> Option<&Edge> {
        self.edges.iter().find(|e| e.from == from && e.when == when)
    }

    /// Every seat of the flow (roles and parallel groups), in node order.
    pub fn seats(&self) -> impl Iterator<Item = &Seat> {
        self.nodes.iter().flat_map(|n| match &n.kind {
            NodeKind::Role(seat) => std::slice::from_ref(seat),
            NodeKind::Parallel { roles } => roles.as_slice(),
            _ => &[],
        })
    }

    /// Everything that keeps the flow from running, in a stable order;
    /// empty when it is valid. `role` finds a role of the library by id.
    pub fn problems(&self, role: &dyn Fn(&str) -> Option<RoleDef>) -> Vec<FlowProblem> {
        let mut problems = Vec::new();
        if !valid_name(&self.name) {
            problems.push(FlowProblem::BadName);
        }
        if self.nodes.len() > MAX_NODES {
            problems.push(FlowProblem::TooManyNodes);
            return problems;
        }
        let mut ids = BTreeSet::new();
        for node in &self.nodes {
            if !valid_node_id(&node.id) {
                problems.push(FlowProblem::BadNodeId(node.id.clone()));
            } else if !ids.insert(node.id.as_str()) {
                problems.push(FlowProblem::DuplicateNode(node.id.clone()));
            }
        }
        let starts = self
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Start)
            .count();
        if starts != 1 {
            problems.push(FlowProblem::Starts(starts));
        }
        if !self.nodes.iter().any(|n| n.kind == NodeKind::End) {
            problems.push(FlowProblem::NoEnd);
        }
        for edge in &self.edges {
            for end in [&edge.from, &edge.to] {
                if !ids.contains(end.as_str()) {
                    problems.push(FlowProblem::UnknownNode(end.clone()));
                }
            }
            if edge.max_rounds.is_some_and(|r| r == 0 || r > MAX_ROUNDS) {
                problems.push(FlowProblem::BadRounds(edge.from.clone()));
            }
        }
        if !problems.is_empty() {
            return problems;
        }
        self.check_connections(&mut problems);
        self.check_steps(role, &mut problems);
        if problems.is_empty() {
            self.check_paths(&mut problems);
        }
        problems
    }

    fn check_connections(&self, problems: &mut Vec<FlowProblem>) {
        for node in &self.nodes {
            let mut out: Vec<When> = self
                .edges
                .iter()
                .filter(|e| e.from == node.id)
                .map(|e| e.when)
                .collect();
            out.sort();
            let wanted: &[When] = match node.kind {
                NodeKind::End => &[],
                NodeKind::Condition(_) => &[When::Pass, When::Fail],
                _ => &[When::After],
            };
            let into_start = self
                .edges
                .iter()
                .any(|e| e.to == node.id && node.kind == NodeKind::Start);
            if out != wanted || into_start {
                problems.push(FlowProblem::Connections(node.id.clone()));
            }
        }
    }

    fn check_steps(&self, role: &dyn Fn(&str) -> Option<RoleDef>, problems: &mut Vec<FlowProblem>) {
        let mut known = HashMap::new();
        let mut def = |name: &str| -> Option<RoleDef> {
            known
                .entry(name.to_owned())
                .or_insert_with(|| role(name))
                .clone()
        };
        let mut unknown = BTreeSet::new();
        for node in &self.nodes {
            let seats: &[Seat] = match &node.kind {
                NodeKind::Role(seat) => std::slice::from_ref(seat),
                NodeKind::Parallel { roles } => {
                    if roles.len() < 2 {
                        problems.push(FlowProblem::SmallParallel(node.id.clone()));
                    }
                    roles
                }
                NodeKind::Condition(Check::Tests {
                    command: Some(command),
                }) => {
                    if command.trim().is_empty() || command.contains(['\n', '\r']) {
                        problems.push(FlowProblem::EmptyCommand(node.id.clone()));
                    }
                    &[]
                }
                NodeKind::Condition(Check::Verdict) => {
                    let feeds = self.edges.iter().filter(|e| e.to == node.id);
                    let mut any = false;
                    let mut all_verdicts = true;
                    for edge in feeds {
                        any = true;
                        let gives = match self.node(&edge.from).map(|n| &n.kind) {
                            Some(NodeKind::Role(seat)) => {
                                def(&seat.role).is_some_and(|r| r.delivers == Delivers::Verdict)
                            }
                            Some(NodeKind::Parallel { roles }) => roles.iter().any(|s| {
                                def(&s.role).is_some_and(|r| r.delivers == Delivers::Verdict)
                            }),
                            _ => false,
                        };
                        all_verdicts &= gives;
                    }
                    if !any || !all_verdicts {
                        problems.push(FlowProblem::NoVerdict(node.id.clone()));
                    }
                    &[]
                }
                _ => &[],
            };
            let parallel = matches!(node.kind, NodeKind::Parallel { .. });
            for seat in seats {
                if !valid_model(&seat.model) {
                    problems.push(FlowProblem::BadSeat(node.id.clone()));
                }
                match def(&seat.role) {
                    None => {
                        if unknown.insert(seat.role.clone()) {
                            problems.push(FlowProblem::UnknownRole(seat.role.clone()));
                        }
                    }
                    Some(r) if parallel && !r.read_only() => {
                        problems.push(FlowProblem::ParallelWriter(r.name.clone()));
                    }
                    Some(_) => {}
                }
            }
        }
    }

    /// Reachability both ways, and no cycle without a bounded connection.
    fn check_paths(&self, problems: &mut Vec<FlowProblem>) {
        let start = self.start().expect("checked").id.as_str();
        let forward = reach(start, |n| {
            self.edges
                .iter()
                .filter(move |e| e.from == n)
                .map(|e| e.to.as_str())
        });
        let mut backward = BTreeSet::new();
        for end in self.nodes.iter().filter(|n| n.kind == NodeKind::End) {
            backward.extend(reach(&end.id, |n| {
                self.edges
                    .iter()
                    .filter(move |e| e.to == n)
                    .map(|e| e.from.as_str())
            }));
        }
        for node in &self.nodes {
            if !forward.contains(node.id.as_str()) {
                problems.push(FlowProblem::Unreachable(node.id.clone()));
            } else if !backward.contains(node.id.as_str()) {
                problems.push(FlowProblem::DeadEnd(node.id.clone()));
            }
        }
        // Without the bounded connections, what is left must have no cycle.
        if let Some(node) = self.unbounded_cycle() {
            problems.push(FlowProblem::UnboundedLoop(node));
        }
    }

    /// A node on a cycle whose connections carry no maximum, if any.
    fn unbounded_cycle(&self) -> Option<String> {
        let free: Vec<&Edge> = self
            .edges
            .iter()
            .filter(|e| e.max_rounds.is_none())
            .collect();
        // Kahn's algorithm: whatever cannot be peeled off sits on a cycle.
        let mut incoming: BTreeMap<&str, usize> =
            self.nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
        for e in &free {
            *incoming.get_mut(e.to.as_str()).expect("checked") += 1;
        }
        let mut ready: Vec<&str> = incoming
            .iter()
            .filter(|(_, n)| **n == 0)
            .map(|(id, _)| *id)
            .collect();
        while let Some(id) = ready.pop() {
            for e in free.iter().filter(|e| e.from == id) {
                let n = incoming.get_mut(e.to.as_str()).expect("checked");
                *n -= 1;
                if *n == 0 {
                    ready.push(e.to.as_str());
                }
            }
            incoming.remove(id);
        }
        // Report the first node (in the flow's order) still on a cycle.
        self.nodes
            .iter()
            .find(|n| incoming.get(n.id.as_str()).is_some_and(|c| *c > 0))
            .map(|n| n.id.clone())
    }
}

fn reach<'a, I>(from: &'a str, next: impl Fn(&'a str) -> I) -> BTreeSet<&'a str>
where
    I: Iterator<Item = &'a str>,
{
    let mut seen = BTreeSet::from([from]);
    let mut todo = vec![from];
    while let Some(id) = todo.pop() {
        for n in next(id) {
            if seen.insert(n) {
                todo.push(n);
            }
        }
    }
    seen
}

/// How a step ended, as far as the path is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    /// A role, an approval or a parallel group finished.
    Done,
    Passed,
    Failed,
}

impl Outcome {
    fn when(self) -> When {
        match self {
            Self::Done => When::After,
            Self::Passed => When::Pass,
            Self::Failed => When::Fail,
        }
    }
}

/// Where a run of a flow is: its node and how many times it took each
/// bounded connection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub node: String,
    /// By connection, `from>to`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rounds: BTreeMap<String, u32>,
}

/// What comes after a step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// Go on to this node.
    Go(String),
    /// The loop back to `to` already ran its `max` rounds: the run stops
    /// and the user decides.
    OutOfRounds { to: String, max: u32 },
}

impl Position {
    pub fn at_start(flow: &Flow) -> Self {
        Self {
            node: flow.start().map(|n| n.id.clone()).unwrap_or_default(),
            rounds: BTreeMap::new(),
        }
    }

    /// The round a loop is on (1 before any loop back), for the user.
    pub fn round(&self) -> u32 {
        self.rounds.values().copied().max().unwrap_or(0) + 1
    }

    /// Moves on from the current node after `outcome`. `None`: the flow has
    /// no such connection (a validated flow always has one, except at its end).
    pub fn advance(&mut self, flow: &Flow, outcome: Outcome) -> Option<Next> {
        let edge = flow.edge(&self.node, outcome.when())?;
        if let Some(max) = edge.max_rounds {
            let key = format!("{}>{}", edge.from, edge.to);
            let taken = self.rounds.entry(key).or_default();
            if *taken >= max {
                return Some(Next::OutOfRounds {
                    to: edge.to.clone(),
                    max,
                });
            }
            *taken += 1;
        }
        self.node = edge.to.clone();
        Some(Next::Go(edge.to.clone()))
    }
}

/// The board section where the user's feedback goes (what to change).
pub const FEEDBACK: &str = "feedback";
/// The pseudo-section of the frame (goal, rules, files, notes) in `Sees`.
pub const FRAME: &str = "frame";
/// Longest text one entry of the board keeps.
pub const MAX_BOARD_TEXT: usize = 16_000;
/// Most entries one run's board keeps: a looping or misled agent cannot
/// grow every later prompt without end.
pub const MAX_BOARD_ENTRIES: usize = 200;
/// Most sections one run's board has.
pub const MAX_BOARD_SECTIONS: usize = 20;
/// Most characters of board the prompt or `context_read` carries; the
/// oldest entries are left out first.
pub const MAX_BOARD_SHOWN: usize = 48_000;

/// The entries that fit in [`MAX_BOARD_SHOWN`], the newest kept.
pub fn board_within<'a>(entries: &[&'a BoardEntry]) -> Vec<&'a BoardEntry> {
    let mut total = 0;
    let mut kept: Vec<&BoardEntry> = entries
        .iter()
        .rev()
        .take_while(|e| {
            total += e.text.len() + e.section.len() + e.role.len();
            total <= MAX_BOARD_SHOWN
        })
        .copied()
        .collect();
    kept.reverse();
    kept
}

/// One entry of a run's blackboard: what a role (or the user) left for the
/// others, in a section (`plan`, `changes`, `tests`, `review`…).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardEntry {
    pub section: String,
    /// Who wrote it, for people (the role's name, or the user).
    pub role: String,
    /// The run's round when it was written (1 before any loop back).
    pub round: u32,
    pub text: String,
    /// The user's own words, not a role's.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_user: bool,
}

/// The board's two tools, as the model sees them (the board server and the
/// API agent): their description and input schema.
pub fn board_tools() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "name": "context_read",
            "description": tr!(
                "Lee la pizarra del equipo: lo último de cada sección que ves, o el historial de una sección.",
                "Reads the team's board: the latest of each section you see, or the history of one section."
            ),
            "input_schema": {
                "type": "object",
                "properties": {
                    "section": {"type": "string", "description": "plan, changes, tests, review, feedback… (optional)"}
                }
            }
        }),
        serde_json::json!({
            "name": "context_post",
            "description": tr!(
                "Deja algo en la pizarra para los demás roles: un hallazgo, una decisión, un resultado. Nunca secretos.",
                "Leaves something on the board for the other roles: a finding, a decision, a result. Never secrets."
            ),
            "input_schema": {
                "type": "object",
                "properties": {
                    "section": {"type": "string", "description": "lowercase-with-dashes"},
                    "text": {"type": "string"}
                },
                "required": ["section", "text"]
            }
        }),
    ]
}

/// A board section name: lowercase letters, digits and dashes.
pub fn valid_section(section: &str) -> bool {
    (1..=MAX_NAME).contains(&section.len())
        && section
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// What a role sees of the board: the latest entry of each section it
/// reads, in the order the sections first appeared (summaries, not the
/// history). The user's feedback is always seen.
pub fn board_view<'a>(entries: &'a [BoardEntry], role: &RoleDef) -> Vec<&'a BoardEntry> {
    let mut order: Vec<&str> = Vec::new();
    for e in entries {
        if !order.contains(&e.section.as_str()) {
            order.push(&e.section);
        }
    }
    order
        .into_iter()
        .filter(|s| role.sees_section(s))
        .filter_map(|s| entries.iter().rev().find(|e| e.section == s))
        .collect()
}

/// A task run by a flow: the flow and its roles as they were when it
/// started (editing the library meanwhile changes nothing here) and where it
/// is. What the roles hand over lives on the run's board (a store table: the
/// board server writes it from another process).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRun {
    pub flow: Flow,
    pub roles: BTreeMap<String, RoleDef>,
    pub position: Position,
    /// What the current role said before its account's limit cut it: it
    /// opens its delivery when it finishes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<String>,
    /// Which role of a group at once works now (they run one after another).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub lane: usize,
    /// The verdicts given since the last verdict condition: `None` for a
    /// role that gave none Hedy could read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verdicts: Vec<Option<bool>>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// A reviewer's verdict from its final answer: the last line that says
/// `VEREDICTO: APRUEBA` / `CAMBIOS` (or `VERDICT: APPROVE` / `CHANGES`), any
/// case. `None` when there is none: Hedy never guesses from free text.
pub fn parse_verdict(text: &str) -> Option<bool> {
    text.lines().rev().find_map(|line| {
        let line = line
            .trim()
            .trim_matches(|c: char| c == '*' || c == '`' || c == '#')
            .trim()
            .to_lowercase();
        let (key, value) = line.split_once(':')?;
        if !matches!(key.trim(), "veredicto" | "verdict") {
            return None;
        }
        match value.trim().trim_end_matches('.') {
            "aprueba" | "approve" | "approved" => Some(true),
            "cambios" | "changes" => Some(false),
            _ => None,
        }
    })
}

impl FlowRun {
    /// A run at the flow's start, with every role it names (`None` when one
    /// is missing: the flow is not runnable).
    pub fn new(flow: Flow, role: &dyn Fn(&str) -> Option<RoleDef>) -> Option<Self> {
        let mut roles = BTreeMap::new();
        for seat in flow.seats() {
            roles.insert(seat.role.clone(), role(&seat.role)?);
        }
        Some(Self {
            position: Position::at_start(&flow),
            flow,
            roles,
            partial: None,
            lane: 0,
            verdicts: Vec::new(),
        })
    }

    pub fn current(&self) -> Option<&Node> {
        self.flow.node(&self.position.node)
    }

    /// The role of the current step, when it is one (in a group at once,
    /// the one working now).
    pub fn current_role(&self) -> Option<&RoleDef> {
        self.roles.get(&self.current_seat()?.role)
    }

    /// Who works on the current step, when a role does.
    pub fn current_seat(&self) -> Option<&Seat> {
        match &self.current()?.kind {
            NodeKind::Role(seat) => Some(seat),
            NodeKind::Parallel { roles } => roles.get(self.lane),
            _ => None,
        }
    }

    /// The step that leads into `node` (the role whose work an approval
    /// looks at).
    pub fn before(&self, node: &str) -> Option<&str> {
        self.flow
            .edges
            .iter()
            .find(|e| e.to == node && e.when == When::After)
            .map(|e| e.from.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Assignment;

    #[test]
    fn a_verdict_is_read_from_its_own_line_never_guessed() {
        assert_eq!(parse_verdict("Bien.\nVEREDICTO: APRUEBA"), Some(true));
        assert_eq!(parse_verdict("**Verdict: changes**"), Some(false));
        assert_eq!(parse_verdict("VEREDICTO: cambios.\n"), Some(false));
        assert_eq!(parse_verdict("Todo bien, apruebo"), None);
        assert_eq!(parse_verdict("VEREDICTO: quizá"), None);
        // The last one counts.
        assert_eq!(
            parse_verdict("VERDICT: CHANGES\nAl final sí.\nVERDICT: APPROVE"),
            Some(true)
        );
    }

    #[test]
    fn a_role_sees_the_latest_of_its_sections_its_own_and_your_feedback() {
        let entry = |section: &str, text: &str, round: u32| BoardEntry {
            section: section.into(),
            role: "x".into(),
            round,
            text: text.into(),
            by_user: false,
        };
        let board = vec![
            entry("plan", "plan 1", 1),
            entry("tests", "2 fallan", 1),
            entry("changes", "arreglo", 2),
            entry("tests", "pasan", 2),
            entry(FEEDBACK, "en el servidor", 1),
        ];
        let reviewer = builtin_role("reviewer").unwrap();
        let seen: Vec<&str> = board_view(&board, &reviewer)
            .iter()
            .map(|e| e.text.as_str())
            .collect();
        assert_eq!(seen, ["plan 1", "pasan", "arreglo", "en el servidor"]);
        let architect = builtin_role("architect").unwrap();
        let seen: Vec<&str> = board_view(&board, &architect)
            .iter()
            .map(|e| e.text.as_str())
            .collect();
        assert_eq!(
            seen,
            ["plan 1", "en el servidor"],
            "its own plan and your feedback"
        );
        assert!(valid_section("riesgos-2") && !valid_section("Plan") && !valid_section(""));
        assert_eq!(builtin_role("tester").unwrap().section(), "tests");
    }

    fn lib(id: &str) -> Option<RoleDef> {
        builtin_role(id)
    }

    fn id(s: &str) -> AccountId {
        AccountId::new(s).unwrap()
    }

    #[test]
    fn the_templates_are_valid() {
        let phases = PhasePlan::from([
            (
                Phase::Plan,
                Assignment {
                    account: id("claude"),
                    model: Some("opus".into()),
                },
            ),
            (
                Phase::Review,
                Assignment {
                    account: id("codex"),
                    model: None,
                },
            ),
        ]);
        for template in Template::ALL {
            let flow = template.flow(&phases);
            assert_eq!(flow.problems(&lib), vec![], "{}", template.key());
            assert_eq!(Template::from_key(template.key()), Some(template));
        }
        let classic = Template::Classic.flow(&phases);
        let roles: Vec<_> = classic.seats().map(|s| s.role.as_str()).collect();
        assert_eq!(roles, vec!["architect", "programmer", "reviewer"]);
        let architect = classic.seats().next().unwrap();
        assert_eq!(architect.account, Some(id("claude")));
        assert_eq!(architect.model.as_deref(), Some("opus"));
    }

    #[test]
    fn the_builtin_roles() {
        let roles = builtin_roles();
        let ids: Vec<_> = roles.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["architect", "programmer", "tester", "reviewer", "security"]
        );
        assert!(roles.iter().all(|r| valid_role_id(&r.id)));
        let reviewer = builtin_role("reviewer").unwrap();
        assert_eq!(reviewer.phase(), Phase::Review);
        assert_eq!(reviewer.delivers, Delivers::Verdict);
        assert_eq!(
            builtin_role("programmer").unwrap().phase(),
            Phase::Implement
        );
    }

    #[test]
    fn a_loop_runs_its_rounds_and_then_stops() {
        let flow = Template::FixLoop.flow(&PhasePlan::new());
        let mut at = Position::at_start(&flow);
        assert_eq!(
            at.advance(&flow, Outcome::Done),
            Some(Next::Go("n2".into()))
        );
        assert_eq!(at.round(), 1);
        // Programmer done, tests fail twice: back to the programmer twice.
        for round in 2..=3 {
            assert_eq!(
                at.advance(&flow, Outcome::Done),
                Some(Next::Go("n3".into()))
            );
            assert_eq!(
                at.advance(&flow, Outcome::Failed),
                Some(Next::Go("n2".into()))
            );
            assert_eq!(at.round(), round);
        }
        at.advance(&flow, Outcome::Done);
        assert_eq!(
            at.advance(&flow, Outcome::Failed),
            Some(Next::OutOfRounds {
                to: "n2".into(),
                max: 2
            })
        );
        assert_eq!(at.node, "n3", "it stays where it stopped");
        // Passing goes on, and the end has no way out.
        assert_eq!(
            at.advance(&flow, Outcome::Passed),
            Some(Next::Go("n4".into()))
        );
        at.advance(&flow, Outcome::Done);
        at.advance(&flow, Outcome::Passed);
        assert_eq!(flow.node(&at.node).unwrap().kind, NodeKind::End);
        assert_eq!(at.advance(&flow, Outcome::Done), None);
    }

    fn straight(kinds: Vec<NodeKind>) -> Flow {
        let mut b = Builder::default();
        for kind in kinds {
            b.node(kind);
        }
        b.finish("x".into())
    }

    #[test]
    fn a_loop_without_rounds_is_refused() {
        let mut flow = Template::FixLoop.flow(&PhasePlan::new());
        assert!(flow.problems(&lib).is_empty());
        for edge in &mut flow.edges {
            edge.max_rounds = None;
        }
        assert_eq!(
            flow.problems(&lib),
            vec![FlowProblem::UnboundedLoop("n2".into())]
        );
        flow.edges[5].max_rounds = Some(11);
        assert!(
            flow.problems(&lib)
                .contains(&FlowProblem::BadRounds("n3".into()))
        );
    }

    #[test]
    fn only_readers_work_in_parallel_and_verdicts_need_a_reviewer() {
        let flow = straight(vec![NodeKind::Parallel {
            roles: vec![Seat::new("reviewer"), Seat::new("programmer")],
        }]);
        assert_eq!(
            flow.problems(&lib),
            vec![FlowProblem::ParallelWriter(
                builtin_role("programmer").unwrap().name
            )]
        );
        let mut flow = Template::FixLoop.flow(&PhasePlan::new());
        // The reviewer before the verdict becomes the tester.
        flow.nodes[3].kind = NodeKind::Role(Seat::new("tester"));
        assert_eq!(
            flow.problems(&lib),
            vec![FlowProblem::NoVerdict("n5".into())]
        );
    }

    #[test]
    fn structural_problems() {
        let mut flow = straight(vec![NodeKind::Role(Seat::new("ghost"))]);
        assert_eq!(
            flow.problems(&lib),
            vec![FlowProblem::UnknownRole("ghost".into())]
        );
        flow.nodes[1].kind = NodeKind::Role(Seat {
            model: Some("--dangerous".into()),
            ..Seat::new("programmer")
        });
        assert_eq!(flow.problems(&lib), vec![FlowProblem::BadSeat("n2".into())]);
        flow.nodes[1].kind = NodeKind::Condition(Check::Tests {
            command: Some("cargo test\nrm -rf /".into()),
        });
        // A condition needs both ways out.
        let problems = flow.problems(&lib);
        assert!(problems.contains(&FlowProblem::EmptyCommand("n2".into())));
        assert!(problems.contains(&FlowProblem::Connections("n2".into())));

        let mut flow = straight(vec![NodeKind::Role(Seat::new("programmer"))]);
        flow.nodes.push(Node {
            id: "lost".into(),
            kind: NodeKind::Approval,
            at: None,
        });
        flow.edges.push(Edge {
            from: "lost".into(),
            to: "n3".into(),
            when: When::After,
            max_rounds: None,
        });
        assert_eq!(
            flow.problems(&lib),
            vec![FlowProblem::Unreachable("lost".into())]
        );
        flow.edges.push(Edge {
            from: "n9".into(),
            to: "n1".into(),
            when: When::After,
            max_rounds: None,
        });
        assert_eq!(
            flow.problems(&lib),
            vec![FlowProblem::UnknownNode("n9".into())]
        );
        let empty = Flow {
            name: " ".into(),
            frame: Frame::default(),
            nodes: vec![],
            edges: vec![],
            triggers: vec![],
        };
        assert_eq!(
            empty.problems(&lib),
            vec![
                FlowProblem::BadName,
                FlowProblem::Starts(0),
                FlowProblem::NoEnd
            ]
        );
    }

    #[test]
    fn a_flow_round_trips_as_json() {
        let flow = Template::DoubleReview.flow(&PhasePlan::new());
        let json = serde_json::to_string(&flow).unwrap();
        assert!(json.contains(r#""kind":"parallel""#), "{json}");
        let back: Flow = serde_json::from_str(&json).unwrap();
        assert_eq!(back, flow);
    }
}
