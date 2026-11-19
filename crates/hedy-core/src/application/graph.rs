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
use crate::tr;

/// Longest label of a note, in characters (the full text opens on click).
const NOTE_LABEL: usize = 120;

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
pub struct MemoryGraph {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
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
        let (label, _) = preview(text, NOTE_LABEL);
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
            let id = self.node(
                &format!("topic:{topic}"),
                GraphKind::Topic,
                &format!("# {topic}"),
                "topic",
            );
            self.link(note, &id);
        }
    }

    fn finish(mut self) -> MemoryGraph {
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
        }
    }
}

fn project_group(path: &std::path::Path) -> String {
    format!("project:{}", path.display())
}

impl Hedy {
    /// The memory as a graph: every approved note of this PC (protected
    /// projects' too: it is only shown here) and every team's this PC
    /// reads.
    pub fn memory_graph(&self) -> AppResult<MemoryGraph> {
        let mut g = Builder::default();
        let you = g.node("you", GraphKind::You, &tr!("Tú", "You"), "you");
        let projects = self.projects()?;
        let teams: Vec<_> = self.teams()?.into_iter().filter(|t| !t.waiting).collect();
        let mut project_ids: BTreeMap<PathBuf, String> = BTreeMap::new();
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
                let team_id = format!("t:{team}");
                g.link(&id, &team_id);
            }
            project_ids.insert(project.path.clone(), id);
        }
        for team in &teams {
            g.node(
                &format!("t:{}", team.id),
                GraphKind::Team,
                &team.name,
                &format!("team:{}", team.id),
            );
        }

        let mut notes = self.store.notes(None, false)?;
        for project in &projects {
            notes.extend(self.store.notes(Some(&project.path), false)?);
        }
        for note in notes
            .into_iter()
            .filter(|n| n.status == NoteStatus::Approved)
        {
            let (owner, group) = match note
                .project
                .as_ref()
                .and_then(|p| project_ids.get(p).map(|id| (id, p)))
            {
                Some((id, path)) => (id.clone(), project_group(path)),
                None => (you.clone(), "you".to_owned()),
            };
            let id = g.note(
                format!("n:{}", note.id),
                &note.text,
                &group,
                SourceRef::Local { id: note.id },
                SourceLayer::Mine,
            );
            g.link(&id, &owner);
            g.topic(&id, note.topic.as_deref());
        }

        for team in &teams {
            let team_node = format!("t:{}", team.id);
            let group = format!("team:{}", team.id);
            // A team this PC cannot read now leaves only its node.
            for note in self.team_notes(&team.id).unwrap_or_default() {
                let layer = if note.team_layer {
                    SourceLayer::Team
                } else {
                    SourceLayer::Project
                };
                let id = g.note(
                    format!("n:{}:{}", team.id, note.uid),
                    &note.text,
                    &group,
                    SourceRef::Team {
                        team: team.id.clone(),
                        uid: note.uid.clone(),
                    },
                    layer,
                );
                let owner = match (&note.project, note.team_layer) {
                    (_, true) => team_node.clone(),
                    (Some(path), false) => match project_ids.get(path) {
                        Some(project) => project.clone(),
                        None => team_node.clone(),
                    },
                    (None, false) => {
                        // A project of the team that is not on this PC.
                        let project = g.node(
                            &format!("tp:{}", team.id),
                            GraphKind::Project,
                            &tr!("Otros proyectos", "Other projects"),
                            &group,
                        );
                        g.link(&project, &team_node);
                        project
                    }
                };
                g.link(&id, &owner);
                let person = g.node(
                    &format!("person:{}:{}", team.id, note.person.as_str()),
                    GraphKind::Person,
                    &note.author,
                    "person",
                );
                g.link(&id, &person);
                g.link(&person, &team_node);
                g.topic(&id, note.topic.as_deref());
            }
        }
        Ok(g.finish())
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
}
