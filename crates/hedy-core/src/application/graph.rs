//! The memory as a graph (docs/decisions.md §36): every note this person
//! can read here, joined to where it lives (a project, a team, every
//! project), to its topic and, in a team, to who wrote it. Only shown on
//! this PC: nothing of it is sent anywhere.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::Serialize;

use super::{AppResult, Hedy};
use crate::domain::NoteStatus;
use crate::domain::ask::{SourceLayer, SourceRef};
use crate::domain::memory::preview;
use crate::domain::sync::MemoryHome;
use crate::domain::team::reveal_hidden;
use crate::tr;

/// Longest label of a note, in characters (the full text opens on click).
const NOTE_LABEL: usize = 120;
/// Longest label of a team, a topic or a person.
const HUB_LABEL: usize = 60;
/// Notes in the graph, at most (the newest): past this, drawing it would
/// stall the screen, and a team member could fill it on purpose.
pub const GRAPH_NOTES_MAX: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphKind {
    /// This person: what their notes for every project hang from.
    You,
    Team,
    Project,
    Topic,
    Person,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: GraphKind,
    pub label: String,
    /// What it is coloured by: `you`, `team:<id>` or `project:<path>`
    /// (a note takes its team's, else its project's).
    pub group: String,
    /// For a note: which one, and its layer.
    pub source: Option<SourceRef>,
    pub layer: Option<SourceLayer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GraphLink {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGraph {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
    /// Older notes left out ([`GRAPH_NOTES_MAX`]).
    pub hidden_notes: usize,
}

/// Builds the graph without repeating a node or a link.
#[derive(Default)]
struct Builder {
    nodes: BTreeMap<String, GraphNode>,
    order: Vec<String>,
    links: BTreeSet<(String, String)>,
}

impl Builder {
    fn node(&mut self, id: &str, kind: GraphKind, label: &str, group: &str) -> String {
        if !self.nodes.contains_key(id) {
            self.order.push(id.to_owned());
            self.nodes.insert(
                id.to_owned(),
                GraphNode {
                    id: id.to_owned(),
                    kind,
                    label: label.to_owned(),
                    group: group.to_owned(),
                    source: None,
                    layer: None,
                },
            );
        }
        id.to_owned()
    }

    fn note(
        &mut self,
        id: String,
        text: &str,
        group: &str,
        source: SourceRef,
        layer: SourceLayer,
    ) -> String {
        let (label, _) = preview(&reveal_hidden(text), NOTE_LABEL);
        self.node(&id, GraphKind::Note, &label, group);
        if let Some(node) = self.nodes.get_mut(&id) {
            node.source = Some(source);
            node.layer = Some(layer);
        }
        id
    }

    fn link(&mut self, a: &str, b: &str) {
        if a != b {
            let pair = if a < b { (a, b) } else { (b, a) };
            self.links.insert((pair.0.to_owned(), pair.1.to_owned()));
        }
    }

    fn topic(&mut self, note: &str, topic: Option<&str>) {
        if let Some(topic) = topic.map(str::trim).filter(|t| !t.is_empty()) {
            let (topic, _) = preview(&reveal_hidden(topic), HUB_LABEL);
            let id = self.node(
                &format!("topic:{topic}"),
                GraphKind::Topic,
                &format!("# {topic}"),
                "topic",
            );
            self.link(note, &id);
        }
    }

    fn finish(mut self, hidden_notes: usize) -> MemoryGraph {
        MemoryGraph {
            nodes: self
                .order
                .iter()
                .filter_map(|id| self.nodes.remove(id))
                .collect(),
            links: self
                .links
                .into_iter()
                .map(|(source, target)| GraphLink { source, target })
                .collect(),
            hidden_notes,
        }
    }
}

fn project_group(path: &std::path::Path) -> String {
    format!("project:{}", path.display())
}

/// A note for the graph, from this PC or a team.
struct GraphNote {
    id: String,
    text: String,
    topic: Option<String>,
    at: i64,
    group: String,
    owner: Owner,
    source: SourceRef,
    layer: SourceLayer,
    /// A team member: their node, name and team's node.
    person: Option<(String, String, String)>,
}

enum Owner {
    You,
    Project(PathBuf),
    Team(String),
    /// A project of the team that is not on this PC.
    TeamElsewhere(String),
}

impl Hedy {
    /// The memory as a graph: every approved note of this PC (protected
    /// projects' too: it is only shown here) and every team's this PC
    /// reads, the newest [`GRAPH_NOTES_MAX`].
    pub fn memory_graph(&self) -> AppResult<MemoryGraph> {
        self.memory_graph_within(GRAPH_NOTES_MAX)
    }

    pub(crate) fn memory_graph_within(&self, max_notes: usize) -> AppResult<MemoryGraph> {
        let projects = self.projects()?;
        let teams: Vec<_> = self.teams()?.into_iter().filter(|t| !t.waiting).collect();
        let mut notes: Vec<GraphNote> = Vec::new();
        let mut local = self.store.notes(None, false)?;
        for project in &projects {
            local.extend(self.store.notes(Some(&project.path), false)?);
        }
        for note in local
            .into_iter()
            .filter(|n| n.status == NoteStatus::Approved)
        {
            let here = note
                .project
                .clone()
                .filter(|p| projects.iter().any(|known| &known.path == p));
            notes.push(GraphNote {
                id: format!("n:{}", note.id),
                group: here
                    .as_deref()
                    .map_or_else(|| "you".to_owned(), project_group),
                owner: here.map_or(Owner::You, Owner::Project),
                text: note.text,
                topic: note.topic,
                at: note.updated_at.unwrap_or(note.created_at),
                source: SourceRef::Local { id: note.id },
                layer: SourceLayer::Mine,
                person: None,
            });
        }
        for team in &teams {
            // A team this PC cannot read now leaves only its node.
            for note in self.team_notes(&team.id).unwrap_or_default() {
                let owner = match (&note.project, note.team_layer) {
                    (_, true) => Owner::Team(team.id.clone()),
                    (Some(path), false) if projects.iter().any(|p| &p.path == path) => {
                        Owner::Project(path.clone())
                    }
                    (Some(_), false) => Owner::Team(team.id.clone()),
                    (None, false) => Owner::TeamElsewhere(team.id.clone()),
                };
                notes.push(GraphNote {
                    id: format!("n:{}:{}", team.id, note.uid),
                    group: format!("team:{}", team.id),
                    owner,
                    text: note.text,
                    topic: note.topic,
                    at: note.updated_at.unwrap_or(note.created_at),
                    source: SourceRef::Team {
                        team: team.id.clone(),
                        uid: note.uid.clone(),
                    },
                    layer: if note.team_layer {
                        SourceLayer::Team
                    } else {
                        SourceLayer::Project
                    },
                    person: Some((
                        format!("person:{}:{}", team.id, note.person.as_str()),
                        note.author,
                        format!("t:{}", team.id),
                    )),
                });
            }
        }
        // This person's notes first, then the newest; a date in the future
        // (a team member's to write) counts as now.
        let now = self.now_secs();
        notes.sort_by_key(|n| {
            (
                !matches!(n.source, SourceRef::Local { .. }),
                std::cmp::Reverse(n.at.min(now)),
            )
        });
        let hidden = notes.len().saturating_sub(max_notes);
        notes.truncate(max_notes);

        let mut g = Builder::default();
        let you = g.node("you", GraphKind::You, &tr!("Tú", "You"), "you");
        for project in &projects {
            let id = g.node(
                &format!("p:{}", project.path.display()),
                GraphKind::Project,
                &project.name,
                &project_group(&project.path),
            );
            if let Ok(Some(MemoryHome::Team(team))) = self.store.memory_home(&project.path)
                && teams.iter().any(|t| t.id == team)
            {
                g.link(&id, &format!("t:{team}"));
            }
        }
        for team in &teams {
            let (name, _) = preview(&reveal_hidden(&team.name), HUB_LABEL);
            g.node(
                &format!("t:{}", team.id),
                GraphKind::Team,
                &name,
                &format!("team:{}", team.id),
            );
        }
        for note in notes {
            let id = g.note(note.id, &note.text, &note.group, note.source, note.layer);
            let owner = match &note.owner {
                Owner::You => you.clone(),
                Owner::Project(path) => format!("p:{}", path.display()),
                Owner::Team(team) => format!("t:{team}"),
                Owner::TeamElsewhere(team) => {
                    let project = g.node(
                        &format!("tp:{team}"),
                        GraphKind::Project,
                        &tr!("Otros proyectos", "Other projects"),
                        &format!("team:{team}"),
                    );
                    g.link(&project, &format!("t:{team}"));
                    project
                }
            };
            g.link(&id, &owner);
            if let Some((person, name, team)) = &note.person {
                // "@": a team member, never taken for "Tú" or a project.
                let (name, _) = preview(&reveal_hidden(name), HUB_LABEL);
                let person = g.node(person, GraphKind::Person, &format!("@{name}"), "person");
                g.link(&id, &person);
                g.link(&person, team);
            }
            g.topic(&id, note.topic.as_deref());
        }
        Ok(g.finish(hidden))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::testing::Fixture;
    use crate::domain::Category;

    fn kinds(g: &MemoryGraph, kind: GraphKind) -> Vec<&str> {
        g.nodes
            .iter()
            .filter(|n| n.kind == kind)
            .map(|n| n.label.as_str())
            .collect()
    }

    fn linked(g: &MemoryGraph, a: &str, b: &str) -> bool {
        g.links
            .iter()
            .any(|l| (l.source == a && l.target == b) || (l.source == b && l.target == a))
    }

    #[test]
    fn notes_hang_from_their_project_or_from_you_and_join_their_topic() {
        let f = Fixture::new();
        f.add_claude("Personal");
        for dir in ["/code/tienda", "/code/agenda"] {
            f.fs.add_git_root(dir);
            f.hedy.use_account(&f.dir(dir), "personal", None).unwrap();
        }
        let tienda = f.dir("/code/tienda");
        let fondo = f
            .hedy
            .add_note(Some(&tienda), "Fondo oscuro #0F1117", Category::Convention)
            .unwrap();
        let global = f
            .hedy
            .add_note(None, "Responde en español", Category::Preference)
            .unwrap();
        f.hedy
            .propose_note(
                &tienda,
                "Fondo negro puro",
                Category::Convention,
                "Claude",
                Some("ui/colores"),
            )
            .unwrap();
        let topic = f
            .hedy
            .propose_note(
                &tienda,
                "Modales sobre el panel",
                Category::Convention,
                "Claude",
                Some("ui/colores"),
            )
            .unwrap();
        f.hedy.approve_note(topic.id).unwrap();

        let g = f.hedy.memory_graph().unwrap();
        assert_eq!(kinds(&g, GraphKind::You), ["Tú"]);
        assert_eq!(kinds(&g, GraphKind::Project), ["tienda", "agenda"]);
        assert_eq!(kinds(&g, GraphKind::Topic), ["# ui/colores"]);
        // Proposals are not memory yet.
        assert_eq!(kinds(&g, GraphKind::Note).len(), 3);
        assert!(!kinds(&g, GraphKind::Note).contains(&"Fondo negro puro"));
        let note = |id: i64| format!("n:{id}");
        let project = format!("p:{}", tienda.display());
        assert!(linked(&g, &note(fondo.id), &project));
        assert!(linked(&g, &note(global.id), "you"));
        assert!(linked(&g, &note(topic.id), "topic:ui/colores"));
        let fondo_node = g.nodes.iter().find(|n| n.id == note(fondo.id)).unwrap();
        assert_eq!(fondo_node.group, format!("project:{}", tienda.display()));
        assert_eq!(fondo_node.source, Some(SourceRef::Local { id: fondo.id }));
        // Every link joins two nodes of the graph.
        assert!(g.links.iter().all(|l| {
            g.nodes.iter().any(|n| n.id == l.source) && g.nodes.iter().any(|n| n.id == l.target)
        }));
    }

    #[test]
    fn a_huge_memory_shows_its_newest_notes_and_says_how_many_are_left_out() {
        let f = Fixture::new();
        for i in 0..(GRAPH_NOTES_MAX + 50) {
            f.clock.advance(1);
            f.hedy
                .add_note(None, &format!("nota {i}"), Category::Convention)
                .unwrap();
        }
        let g = f.hedy.memory_graph().unwrap();
        let notes = kinds(&g, GraphKind::Note);
        assert_eq!(notes.len(), GRAPH_NOTES_MAX);
        assert_eq!(g.hidden_notes, 50);
        assert!(notes.contains(&format!("nota {}", GRAPH_NOTES_MAX + 49).as_str()));
        assert!(!notes.contains(&"nota 0"));
    }

    #[test]
    fn a_topic_label_is_short_and_shows_hidden_characters() {
        let mut g = Builder::default();
        let n = g.note(
            "n:1".into(),
            "x",
            "you",
            SourceRef::Local { id: 1 },
            SourceLayer::Mine,
        );
        g.topic(&n, Some(&format!("{}\u{202e}", "a".repeat(100_000))));
        let g = g.finish(0);
        assert!(g.nodes.iter().all(|n| n.label.chars().count() <= 70));
    }
}
