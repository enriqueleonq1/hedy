use std::collections::BTreeMap;

use serde::Serialize;

use crate::Layer;

/// One agent run of one case with one layer. Written as a JSON line.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub case: String,
    pub layer: Layer,
    pub attempt: u32,
    pub account: String,
    pub model: Option<String>,
    /// The check passed.
    pub passed: bool,
    pub tokens: u64,
    /// The CLI's estimate (or the API price); not what a subscription bills.
    pub cost_usd: Option<f64>,
    pub duration_ms: u64,
    /// Files edited, commands run and other tool uses.
    pub actions: u32,
    /// Memory notes the agent read.
    pub notes_read: usize,
    /// Why the run did not get to the check (timeout, launch error…).
    pub error: Option<String>,
    /// The end of the check's output.
    pub check_output: String,
    pub started_at: i64,
}

/// Averages for one case and layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub case: String,
    pub layer: Layer,
    pub runs: usize,
    pub passed: usize,
    pub avg_tokens: u64,
    pub avg_cost_usd: Option<f64>,
    pub avg_secs: f64,
}

pub fn summarize(results: &[RunResult]) -> Vec<Summary> {
    let mut groups: BTreeMap<(String, Layer), Vec<&RunResult>> = BTreeMap::new();
    for r in results {
        groups.entry((r.case.clone(), r.layer)).or_default().push(r);
    }
    groups
        .into_iter()
        .map(|((case, layer), runs)| {
            let n = runs.len();
            let costs: Vec<f64> = runs.iter().filter_map(|r| r.cost_usd).collect();
            Summary {
                case,
                layer,
                runs: n,
                passed: runs.iter().filter(|r| r.passed).count(),
                avg_tokens: runs.iter().map(|r| r.tokens).sum::<u64>() / n as u64,
                avg_cost_usd: (!costs.is_empty())
                    .then(|| costs.iter().sum::<f64>() / costs.len() as f64),
                avg_secs: runs.iter().map(|r| r.duration_ms).sum::<u64>() as f64
                    / n as f64
                    / 1000.0,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(layer: Layer, passed: bool, tokens: u64) -> RunResult {
        RunResult {
            case: "precios".into(),
            layer,
            attempt: 1,
            account: "team".into(),
            model: Some("haiku".into()),
            passed,
            tokens,
            cost_usd: Some(0.02),
            duration_ms: 3000,
            actions: 2,
            notes_read: 0,
            error: None,
            check_output: String::new(),
            started_at: 0,
        }
    }

    #[test]
    fn averages_per_case_and_layer() {
        let summary = summarize(&[
            run(Layer::Memory, true, 100),
            run(Layer::Plain, false, 300),
            run(Layer::Memory, false, 200),
        ]);
        assert_eq!(summary.len(), 2);
        assert_eq!(summary[0].layer, Layer::Plain);
        let memory = &summary[1];
        assert_eq!((memory.runs, memory.passed, memory.avg_tokens), (2, 1, 150));
        assert_eq!(memory.avg_secs, 3.0);
        let json = serde_json::to_value(run(Layer::CodeMap, true, 1)).unwrap();
        assert_eq!(json["layer"], "code-map");
        assert_eq!(json["costUsd"], 0.02);
    }
}
