//! Usage and cost per project and per client.
//!
//! Usage is tokens. The cost in a summary only counts API-key accounts, where
//! usage is really paid, and is an estimate from the price the user configured;
//! a subscription's CLI-reported cost is not a bill and is left out.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;

use super::{DomainError, UsageSample};

/// USD per million tokens, as the provider publishes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ApiPrice {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

impl ApiPrice {
    /// Cached input is charged as regular input: an upper bound.
    pub fn cost(&self, usage: &UsageSample) -> f64 {
        let input = usage.input_tokens + usage.cache_read_tokens + usage.cache_write_tokens;
        (input as f64 * self.input_per_mtok + usage.output_tokens as f64 * self.output_per_mtok)
            / 1_000_000.0
    }
}

impl FromStr for ApiPrice {
    type Err = DomainError;

    /// `"0.60/2.50"`: input / output, USD per million tokens.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || DomainError::InvalidPrice(s.to_owned());
        let (input, output) = s.split_once('/').ok_or_else(invalid)?;
        let parse = |v: &str| {
            v.trim()
                .replace(',', ".")
                .parse::<f64>()
                .ok()
                .filter(|p| p.is_finite() && *p >= 0.0)
                .ok_or_else(invalid)
        };
        Ok(Self {
            input_per_mtok: parse(input)?,
            output_per_mtok: parse(output)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectUsage {
    pub project: PathBuf,
    pub name: String,
    pub client: Option<String>,
    pub usage: UsageSample,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClientUsage {
    /// `None`: projects without a client.
    pub client: Option<String>,
    pub projects: usize,
    pub usage: UsageSample,
}

/// Totals per client, the busiest first and projects without a client last.
pub fn by_client(projects: &[ProjectUsage]) -> Vec<ClientUsage> {
    let mut clients: BTreeMap<Option<String>, ClientUsage> = BTreeMap::new();
    for p in projects {
        let entry = clients
            .entry(p.client.clone())
            .or_insert_with(|| ClientUsage {
                client: p.client.clone(),
                projects: 0,
                usage: UsageSample::default(),
            });
        entry.projects += 1;
        entry.usage.add(&p.usage);
    }
    let mut list: Vec<ClientUsage> = clients.into_values().collect();
    list.sort_by(|a, b| {
        a.client
            .is_none()
            .cmp(&b.client.is_none())
            .then(b.usage.total_tokens().cmp(&a.usage.total_tokens()))
    });
    list
}

/// Fewer tasks than this on either side: too few to compare.
pub const MIN_TASKS_TO_COMPARE: usize = 5;

/// Tokens per task with and without one of Hedy's layers (the memory, the
/// code map), as medians: one huge task does not move a median the way it
/// moves an average.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerComparison {
    /// Fewer than [`MIN_TASKS_TO_COMPARE`] tasks on one side: no figure.
    NotEnough { with: usize, without: usize },
    Medians {
        with: u64,
        without: u64,
        with_tasks: usize,
        without_tasks: usize,
    },
}

/// The middle value (the mean of the two middle ones for an even count).
pub fn median(values: &[u64]) -> Option<u64> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    match n {
        0 => None,
        _ if n % 2 == 1 => Some(sorted[n / 2]),
        _ => Some((sorted[n / 2 - 1] + sorted[n / 2]) / 2),
    }
}

/// Compares the tokens of tasks that had a layer (`true`) with those that
/// did not.
pub fn compare_layer(tasks: &[(bool, u64)]) -> LayerComparison {
    let side = |had: bool| -> Vec<u64> {
        tasks
            .iter()
            .filter(|(h, _)| *h == had)
            .map(|(_, t)| *t)
            .collect()
    };
    let (with, without) = (side(true), side(false));
    match (median(&with), median(&without)) {
        (Some(w), Some(wo))
            if with.len() >= MIN_TASKS_TO_COMPARE && without.len() >= MIN_TASKS_TO_COMPARE =>
        {
            LayerComparison::Medians {
                with: w,
                without: wo,
                with_tasks: with.len(),
                without_tasks: without.len(),
            }
        }
        _ => LayerComparison::NotEnough {
            with: with.len(),
            without: without.len(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medians_resist_one_huge_task() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[7]), Some(7));
        assert_eq!(median(&[5, 1, 900_000]), Some(5));
        assert_eq!(median(&[4, 1, 3, 2]), Some(2));
    }

    #[test]
    fn compares_only_with_enough_tasks_on_each_side() {
        let mut tasks: Vec<(bool, u64)> = (1..=5).map(|i| (true, i * 100)).collect();
        tasks.extend((1..=4).map(|i| (false, i * 1000)));
        assert_eq!(
            compare_layer(&tasks),
            LayerComparison::NotEnough {
                with: 5,
                without: 4
            }
        );
        tasks.push((false, 5000));
        assert_eq!(
            compare_layer(&tasks),
            LayerComparison::Medians {
                with: 300,
                without: 3000,
                with_tasks: 5,
                without_tasks: 5
            }
        );
        assert_eq!(
            compare_layer(&[]),
            LayerComparison::NotEnough {
                with: 0,
                without: 0
            }
        );
    }

    fn usage(tokens: u64, cost: Option<f64>) -> UsageSample {
        UsageSample {
            input_tokens: tokens,
            cost_usd: cost,
            ..UsageSample::default()
        }
    }

    #[test]
    fn prices_parse_and_apply() {
        let price: ApiPrice = "0,60 / 2.5".parse().unwrap();
        assert_eq!(price.input_per_mtok, 0.6);
        let sample = UsageSample {
            input_tokens: 1_000_000,
            output_tokens: 200_000,
            ..UsageSample::default()
        };
        assert!((price.cost(&sample) - 1.1).abs() < 1e-9);
        assert!("gratis".parse::<ApiPrice>().is_err());
        assert!("1/-2".parse::<ApiPrice>().is_err());
    }

    #[test]
    fn groups_projects_by_client() {
        let project = |name: &str, client: Option<&str>, tokens, cost| ProjectUsage {
            project: PathBuf::from(format!("/code/{name}")),
            name: name.into(),
            client: client.map(str::to_owned),
            usage: usage(tokens, cost),
        };
        let clients = by_client(&[
            project("agenda", Some("Estudio Norte"), 100, Some(1.0)),
            project("hedy", None, 900, None),
            project("tienda", Some("Café Aurora"), 500, None),
            project("citas", Some("Estudio Norte"), 50, Some(0.5)),
        ]);
        let names: Vec<_> = clients.iter().map(|c| c.client.as_deref()).collect();
        assert_eq!(names, [Some("Café Aurora"), Some("Estudio Norte"), None]);
        assert_eq!(clients[1].projects, 2);
        assert_eq!(clients[1].usage.total_tokens(), 150);
        assert_eq!(clients[1].usage.cost_usd, Some(1.5));
    }
}
