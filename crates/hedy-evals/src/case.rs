use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use hedy_core::domain::Category;
use hedy_core::experiments::{Experiment, Experiments};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteSpec {
    pub text: String,
    pub category: Category,
}

/// `case.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaseSpec {
    pub title: String,
    /// What the user asks, as they would in the chat.
    pub prompt: String,
    /// Shell command run in the worktree after the agent finishes; exit 0 passes.
    pub check: String,
    /// Project memory for the layers that include it.
    #[serde(default)]
    pub notes: Vec<NoteSpec>,
    /// Team-layer notes (docs/decisions.md §35) for the layers with memory:
    /// the project joins a team made for the run, and the agent reads them
    /// always, quoted.
    #[serde(default)]
    pub team_notes: Vec<NoteSpec>,
    /// Which layer the case is meant to measure (informative).
    #[serde(default)]
    pub measures: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    900
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    /// Folder name.
    pub name: String,
    pub dir: PathBuf,
    pub spec: CaseSpec,
}

impl Case {
    pub fn repo(&self) -> PathBuf {
        self.dir.join("repo")
    }

    pub fn checks(&self) -> PathBuf {
        self.dir.join("check")
    }
}

/// Every case in `suite` (folders with a `case.json`), by name.
pub fn load_suite(suite: &Path) -> Result<Vec<Case>> {
    let mut cases = Vec::new();
    let entries = std::fs::read_dir(suite)
        .with_context(|| format!("no se pudo leer la carpeta de evals {}", suite.display()))?;
    for entry in entries {
        let dir = entry?.path();
        let file = dir.join("case.json");
        if !file.is_file() {
            continue;
        }
        let spec: CaseSpec = serde_json::from_str(&std::fs::read_to_string(&file)?)
            .with_context(|| format!("{} no es válido", file.display()))?;
        if !dir.join("repo").is_dir() {
            bail!("al caso {} le falta la carpeta repo/", dir.display());
        }
        cases.push(Case {
            name: dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            dir,
            spec,
        });
    }
    cases.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(cases)
}

/// The cases named in `wanted` (all of them when it is empty), each once even
/// when named twice. Fails naming the ones not in `cases`.
pub fn select_cases(mut cases: Vec<Case>, wanted: &[String]) -> Result<Vec<Case>> {
    if wanted.is_empty() {
        return Ok(cases);
    }
    let mut missing: Vec<&str> = Vec::new();
    for name in wanted {
        if !cases.iter().any(|c| &c.name == name) && !missing.contains(&name.as_str()) {
            missing.push(name);
        }
    }
    if !missing.is_empty() {
        bail!("no encontré estos casos: {}", missing.join(", "));
    }
    cases.retain(|c| wanted.contains(&c.name));
    Ok(cases)
}

/// Which of Hedy's layers a run gets.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum Layer {
    /// The agent alone.
    Plain,
    Memory,
    CodeMap,
    /// Memory and code map.
    Full,
}

impl Layer {
    pub const ALL: [Layer; 4] = [Self::Plain, Self::Memory, Self::CodeMap, Self::Full];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Memory => "memory",
            Self::CodeMap => "code-map",
            Self::Full => "full",
        }
    }

    pub fn memory(self) -> bool {
        matches!(self, Self::Memory | Self::Full)
    }

    pub fn code_map(self) -> bool {
        matches!(self, Self::CodeMap | Self::Full)
    }
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A set of Hedy's experiments a run gets (docs/decisions.md §33), by the
/// name it was asked with: `base` (Hedy as it ships), `none` (without any),
/// `read-guard`, `base+read-guard`… `trim` stands for both trims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub name: String,
    pub experiments: Experiments,
}

impl FromStr for Variant {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let expanded = s
            .split('+')
            .map(|part| match part.trim() {
                "trim" => "trim-guidance+trim-tools",
                other => other,
            })
            .collect::<Vec<_>>()
            .join("+");
        let (experiments, unknown) = Experiments::parse(&expanded);
        if !unknown.is_empty() {
            let known: Vec<&str> = Experiment::ALL.iter().map(|e| e.name()).collect();
            return Err(format!(
                "experimento desconocido: {} (base, trim, {})",
                unknown.join(", "),
                known.join(", ")
            ));
        }
        Ok(Self {
            name: s.trim().to_owned(),
            experiments,
        })
    }
}

impl FromStr for Layer {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|l| l.as_str() == s)
            .ok_or_else(|| format!("capa desconocida: {s} (plain, memory, code-map o full)"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_cases_and_rejects_bad_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let case = tmp.path().join("b-precios");
        std::fs::create_dir_all(case.join("repo")).unwrap();
        std::fs::write(
            case.join("case.json"),
            r#"{"title": "Precios", "prompt": "Formatea", "check": "true",
                "notes": [{"text": "USD 12.50", "category": "convention"}]}"#,
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("a-not-a-case")).unwrap();
        let cases = load_suite(tmp.path()).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].name, "b-precios");
        assert_eq!(cases[0].spec.timeout_secs, 900);
        assert_eq!(cases[0].spec.notes[0].category, Category::Convention);

        std::fs::write(case.join("case.json"), r#"{"title": "x"}"#).unwrap();
        assert!(load_suite(tmp.path()).is_err());
    }

    #[test]
    fn selects_cases_once_and_names_the_missing_ones() {
        let case = |name: &str| Case {
            name: name.into(),
            dir: PathBuf::from(name),
            spec: CaseSpec {
                title: name.into(),
                prompt: String::new(),
                check: "true".into(),
                notes: Vec::new(),
                team_notes: Vec::new(),
                measures: None,
                timeout_secs: 1,
            },
        };
        let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let all = vec![case("a"), case("b")];

        assert_eq!(select_cases(all.clone(), &[]).unwrap().len(), 2);
        let picked = select_cases(all.clone(), &names(&["a", "a"])).unwrap();
        assert_eq!(picked, vec![case("a")]);
        let error = select_cases(all, &names(&["a", "x", "y", "x"])).unwrap_err();
        assert_eq!(error.to_string(), "no encontré estos casos: x, y");
    }

    #[test]
    fn variants() {
        let base: Variant = "base".parse().unwrap();
        assert_eq!(base.name, "base");
        assert_eq!(base.experiments, Experiments::DEFAULT);
        assert!("none".parse::<Variant>().unwrap().experiments.is_empty());
        let trim: Variant = "trim".parse().unwrap();
        assert_eq!(trim.name, "trim");
        assert_eq!(trim.experiments.names(), "trim-guidance,trim-tools");
        let both: Variant = "read-guard+briefing".parse().unwrap();
        assert!(both.experiments.has(Experiment::ReadGuard));
        assert!(both.experiments.has(Experiment::Briefing));
        assert!(
            "read-gard"
                .parse::<Variant>()
                .unwrap_err()
                .contains("read-gard")
        );
    }

    #[test]
    fn layers() {
        assert_eq!("code-map".parse::<Layer>(), Ok(Layer::CodeMap));
        assert!(Layer::Full.memory() && Layer::Full.code_map());
        assert!(!Layer::Plain.memory() && !Layer::Plain.code_map());
        assert!("todo".parse::<Layer>().is_err());
    }
}
