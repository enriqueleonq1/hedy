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

#[cfg(test)]
mod tests {
    use super::*;

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
