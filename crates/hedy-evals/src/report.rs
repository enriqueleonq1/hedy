use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Layer;

/// The variant without experiments.
pub const BASE: &str = "base";

/// One step of a flow's run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StepTrace {
    /// A role's turn: its node, its role and what it spent.
    Role {
        node: String,
        role: String,
        tokens: u64,
        #[serde(default)]
        cost_usd: Option<f64>,
    },
    /// A condition and whether it passed.
    Check {
        node: String,
        what: String,
        passed: bool,
    },
}

/// One agent run of one case with one layer and one variant. Written as a
/// JSON line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub case: String,
    pub layer: Layer,
    /// The experiments the run had (`base`: none; e.g. `read-guard` or
    /// `trim-guidance+trim-tools`). Runs from before variants are `base`.
    #[serde(default = "base")]
    pub variant: String,
    pub attempt: u32,
    pub account: String,
    pub model: Option<String>,
    /// The check passed.
    pub passed: bool,
    pub tokens: u64,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
    /// The CLI's estimate (or the API price); not what a subscription bills.
    pub cost_usd: Option<f64>,
    pub duration_ms: u64,
    /// Files edited, commands run and other tool uses.
    pub actions: u32,
    /// `Read` calls, and those of a file already read in the run.
    #[serde(default)]
    pub files_read: u32,
    #[serde(default)]
    pub rereads: u32,
    /// Tool calls Hedy's hook denied (protected files, the read guard).
    #[serde(default)]
    pub denied: u32,
    /// Subagents the agent started (Claude's `Task`: Explore, Plan…).
    #[serde(default)]
    pub subagents: u32,
    /// Memory notes the agent read.
    pub notes_read: usize,
    /// Why the run did not get to the check (timeout, launch error…).
    pub error: Option<String>,
    /// A flow's run, step by step: each role's turn and what it spent, each
    /// check and its result (to judge which steps add something).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<StepTrace>,
    /// The end of the check's output.
    pub check_output: String,
    pub started_at: i64,
}

fn base() -> String {
    BASE.into()
}

/// Medians for one case, layer and variant.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub case: String,
    pub layer: Layer,
    pub variant: String,
    pub runs: usize,
    pub passed: usize,
    pub median_tokens: u64,
    /// Input and cache tokens: what the context costs.
    pub median_input: u64,
    pub median_output: u64,
    pub median_files_read: u64,
    /// The CLI's cost estimate, which weighs each kind of token by its price
    /// (cache writes 1.25–2×, cache reads 0.1×, output 5×): what the
    /// comparison goes by. 0 when the CLI gave none.
    pub median_cost_usd: f64,
    pub avg_cost_usd: Option<f64>,
    pub total_cost_usd: f64,
    /// All the runs' cost over the runs that passed: what a solved task costs.
    pub cost_per_pass: Option<f64>,
    pub avg_secs: f64,
}

/// The middle value (the mean of the two middle ones for an even count).
pub fn median(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    match n {
        0 => 0,
        _ if n % 2 == 1 => sorted[n / 2],
        _ => (sorted[n / 2 - 1] + sorted[n / 2]) / 2,
    }
}

pub fn summarize(results: &[RunResult]) -> Vec<Summary> {
    let mut groups: BTreeMap<(String, Layer, String), Vec<&RunResult>> = BTreeMap::new();
    for r in results {
        groups
            .entry((r.case.clone(), r.layer, r.variant.clone()))
            .or_default()
            .push(r);
    }
    groups
        .into_iter()
        .map(|((case, layer, variant), runs)| {
            let n = runs.len();
            let costs: Vec<f64> = runs.iter().filter_map(|r| r.cost_usd).collect();
            let of =
                |f: fn(&RunResult) -> u64| median(&runs.iter().map(|r| f(r)).collect::<Vec<_>>());
            Summary {
                case,
                layer,
                variant,
                runs: n,
                passed: runs.iter().filter(|r| r.passed).count(),
                median_tokens: of(|r| r.tokens),
                median_input: of(|r| r.input_tokens + r.cache_read_tokens + r.cache_write_tokens),
                median_output: of(|r| r.output_tokens),
                median_files_read: of(|r| u64::from(r.files_read)),
                median_cost_usd: median_f64(
                    &mut runs
                        .iter()
                        .map(|r| r.cost_usd.unwrap_or(0.0))
                        .collect::<Vec<_>>(),
                ),
                avg_cost_usd: (!costs.is_empty())
                    .then(|| costs.iter().sum::<f64>() / costs.len() as f64),
                total_cost_usd: costs.iter().sum(),
                cost_per_pass: cost_per_pass(&runs),
                avg_secs: runs.iter().map(|r| r.duration_ms).sum::<u64>() as f64
                    / n as f64
                    / 1000.0,
            }
        })
        .collect()
}

fn cost_per_pass(runs: &[&RunResult]) -> Option<f64> {
    let passed = runs.iter().filter(|r| r.passed).count();
    let cost: f64 = runs.iter().filter_map(|r| r.cost_usd).sum();
    (passed > 0 && cost > 0.0).then(|| cost / passed as f64)
}

/// One variant against the base, over the cases both ran (same layer).
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub variant: String,
    pub layer: Layer,
    /// Cases compared.
    pub cases: usize,
    pub runs: usize,
    pub passed: usize,
    pub base_runs: usize,
    pub base_passed: usize,
    /// Median over the cases of the change in median tokens, in percent
    /// (negative: fewer tokens than the base).
    pub token_change_pct: f64,
    /// The same, for the cost estimate: what the verdict goes by.
    pub cost_change_pct: f64,
    /// Cost per solved task over all the compared cases, against the base's.
    pub cost_per_pass_change_pct: Option<f64>,
}

impl Comparison {
    pub fn pass_rate(&self) -> f64 {
        rate(self.passed, self.runs)
    }

    pub fn base_pass_rate(&self) -> f64 {
        rate(self.base_passed, self.base_runs)
    }

    /// The rule fixed before the screening (docs/decisions.md §33): 10 % or
    /// more cheaper (median cost; the screening went by tokens, see §33) and
    /// no lower pass rate.
    pub fn verdict(&self) -> Verdict {
        if self.pass_rate() + 1e-9 < self.base_pass_rate() {
            Verdict::Drop
        } else if self.cost_change_pct <= -10.0 {
            Verdict::Keep
        } else if self.cost_change_pct < 0.0 {
            Verdict::Maybe
        } else {
            Verdict::Drop
        }
    }
}

fn rate(passed: usize, runs: usize) -> f64 {
    if runs == 0 {
        0.0
    } else {
        passed as f64 / runs as f64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Saves 10 % or more without failing more.
    Keep,
    /// Saves something, below the bar: maybe only for some cases.
    Maybe,
    /// Saves nothing, or fails more.
    Drop,
}

/// Every variant other than the base against the base, per layer.
pub fn compare(summaries: &[Summary]) -> Vec<Comparison> {
    let mut out = Vec::new();
    let mut variants: Vec<(Layer, &str)> = summaries
        .iter()
        .filter(|s| s.variant != BASE)
        .map(|s| (s.layer, s.variant.as_str()))
        .collect();
    variants.sort();
    variants.dedup();
    for (layer, variant) in variants {
        let mut changes = Vec::new();
        let mut cost_changes = Vec::new();
        let (mut cost, mut base_cost) = (0.0, 0.0);
        let mut c = Comparison {
            variant: variant.to_owned(),
            layer,
            cases: 0,
            runs: 0,
            passed: 0,
            base_runs: 0,
            base_passed: 0,
            token_change_pct: 0.0,
            cost_change_pct: 0.0,
            cost_per_pass_change_pct: None,
        };
        for s in summaries
            .iter()
            .filter(|s| s.layer == layer && s.variant == variant)
        {
            let Some(base) = summaries
                .iter()
                .find(|b| b.layer == layer && b.variant == BASE && b.case == s.case)
            else {
                continue;
            };
            c.cases += 1;
            c.runs += s.runs;
            c.passed += s.passed;
            c.base_runs += base.runs;
            c.base_passed += base.passed;
            changes.push(pct(s.median_tokens as f64, base.median_tokens as f64));
            cost_changes.push(pct(s.median_cost_usd, base.median_cost_usd));
            cost += s.total_cost_usd;
            base_cost += base.total_cost_usd;
        }
        if c.cases == 0 {
            continue;
        }
        c.token_change_pct = median_f64(&mut changes);
        c.cost_change_pct = median_f64(&mut cost_changes);
        // Failed runs cost too: spread over the solved ones.
        c.cost_per_pass_change_pct = (c.passed > 0 && c.base_passed > 0 && base_cost > 0.0)
            .then(|| pct(cost / c.passed as f64, base_cost / c.base_passed as f64));
        out.push(c);
    }
    out
}

fn pct(value: f64, base: f64) -> f64 {
    if base == 0.0 {
        0.0
    } else {
        (value / base - 1.0) * 100.0
    }
}

fn median_f64(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    match n {
        0 => 0.0,
        _ if n % 2 == 1 => values[n / 2],
        _ => (values[n / 2 - 1] + values[n / 2]) / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(case: &str, variant: &str, passed: bool, tokens: u64) -> RunResult {
        RunResult {
            case: case.into(),
            layer: Layer::Memory,
            variant: variant.into(),
            attempt: 1,
            account: "personal".into(),
            model: Some("haiku".into()),
            passed,
            tokens,
            input_tokens: tokens / 2,
            output_tokens: 10,
            cache_read_tokens: tokens / 2,
            cache_write_tokens: 0,
            cost_usd: Some(tokens as f64 / 100_000.0),
            duration_ms: 3000,
            actions: 2,
            files_read: 3,
            rereads: 1,
            denied: 0,
            subagents: 0,
            notes_read: 0,
            error: None,
            check_output: String::new(),
            steps: Vec::new(),
            started_at: 0,
        }
    }

    #[test]
    fn medians_per_case_layer_and_variant() {
        let summary = summarize(&[
            run("precios", BASE, true, 100),
            run("precios", BASE, false, 300),
            run("precios", BASE, true, 900_000),
            run("precios", "read-guard", true, 80),
        ]);
        assert_eq!(summary.len(), 2);
        let base = &summary[0];
        assert_eq!(
            (base.variant.as_str(), base.runs, base.passed),
            (BASE, 3, 2)
        );
        assert_eq!(base.median_tokens, 300);
        assert_eq!(base.median_files_read, 3);
        assert_eq!(base.avg_secs, 3.0);
        // $9.004 over 2 passes.
        assert!((base.cost_per_pass.unwrap() - 4.502).abs() < 1e-9);
        assert_eq!(median(&[4, 1, 3, 2]), 2);
    }

    #[test]
    fn compares_each_variant_with_the_base_and_applies_the_rule() {
        let results = [
            run("a", BASE, true, 1000),
            run("b", BASE, true, 2000),
            run("a", "read-guard", true, 800),
            run("b", "read-guard", true, 1900),
            run("a", "briefing", false, 500),
            run("b", "briefing", true, 1000),
            run("a", "trim", true, 980),
            run("b", "trim", true, 1960),
        ];
        let comparisons = compare(&summarize(&results));
        let get = |v: &str| comparisons.iter().find(|c| c.variant == v).unwrap();
        // -20 % and -5 %: the median of two is -12.5 %.
        assert!((get("read-guard").token_change_pct + 12.5).abs() < 1e-9);
        assert!((get("read-guard").cost_change_pct + 12.5).abs() < 1e-9);
        assert_eq!(get("read-guard").verdict(), Verdict::Keep);
        // Cheaper, but it failed a case the base passed.
        assert_eq!(get("briefing").verdict(), Verdict::Drop);
        assert_eq!(get("trim").verdict(), Verdict::Maybe);
        assert_eq!(get("trim").cases, 2);
        // Half the cost per run, but half the runs failed: same per solved task.
        let failed = get("briefing").cost_per_pass_change_pct.unwrap();
        assert!((failed + 0.0).abs() < 1e-9, "{failed}");

        // Fewer tokens that cost more (cache writes): the cost decides.
        let mut cache = run("a", "cache", true, 500);
        cache.cost_usd = Some(0.02);
        let mut results = results.to_vec();
        results.push(cache);
        let comparisons = compare(&summarize(&results));
        let cache = comparisons.iter().find(|c| c.variant == "cache").unwrap();
        assert!(cache.token_change_pct < 0.0 && cache.cost_change_pct > 0.0);
        assert_eq!(cache.verdict(), Verdict::Drop);
    }

    #[test]
    fn reads_old_lines_as_the_base() {
        let line = r#"{"case":"price-format","layer":"plain","attempt":1,"account":"team","model":"haiku","passed":true,"tokens":5,"costUsd":null,"durationMs":1,"actions":1,"notesRead":0,"error":null,"checkOutput":"","startedAt":1}"#;
        let old: RunResult = serde_json::from_str(line).unwrap();
        assert_eq!((old.variant.as_str(), old.files_read), (BASE, 0));
        let json = serde_json::to_value(run("a", "trim", true, 1)).unwrap();
        assert_eq!(json["layer"], "memory");
        assert_eq!(json["cacheReadTokens"], 0);
    }
}
