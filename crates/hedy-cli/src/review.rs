//! `hedy review`: the change against the written rules (docs/decisions.md
//! §37), each break on its line and citing its note. Only a warning: it
//! always exits 0, so it can sit in a hook without blocking anything.

use std::path::Path;

use anyhow::Result;
use hedy_core::application::{Hedy, RuleCheck, RuleTarget};
use hedy_core::domain::TaskId;
use hedy_core::tr;
use serde_json::json;

use crate::memory_ask::place;
use crate::spinner;
use crate::style::Style;
use crate::team::clean;

/// What `hedy review` was asked for.
#[derive(Debug, Clone, Default)]
pub struct ReviewOptions {
    /// A range (`main..HEAD`) or one commit; `None`: what is not committed.
    pub revs: Option<String>,
    pub staged: bool,
    pub task: Option<String>,
    pub account: Option<String>,
    pub model: Option<String>,
    pub json: bool,
}

pub fn review(hedy: &Hedy, dir: &Path, options: ReviewOptions) -> Result<()> {
    let target = if let Some(task) = &options.task {
        RuleTarget::Task(TaskId::new(task.as_str()))
    } else if options.staged {
        RuleTarget::Staged(dir.to_owned())
    } else if let Some(revs) = &options.revs {
        RuleTarget::Revs {
            dir: dir.to_owned(),
            revs: revs.clone(),
        }
    } else {
        RuleTarget::Work(dir.to_owned())
    };
    let check = spinner::while_running(
        &tr!(
            "Revisando el cambio contra tus reglas…",
            "Checking the change against your rules…"
        ),
        || {
            hedy.check_rules(
                &target,
                options.account.as_deref(),
                options.model.as_deref(),
            )
        },
    )?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&as_json(&check))?);
    } else {
        print_check(&check);
    }
    Ok(())
}

fn as_json(check: &RuleCheck) -> serde_json::Value {
    let rules: Vec<serde_json::Value> = check
        .rules
        .iter()
        .enumerate()
        .map(|(i, rule)| {
            let mut value = serde_json::to_value(rule).unwrap_or_default();
            value["n"] = json!(i + 1);
            value
        })
        .collect();
    json!({
        "breaks": check.breaks,
        "rules": rules,
        "files": check.files,
        "protected": check.protected,
        "truncated": check.truncated,
        "account": check.account.as_ref().map(|a| a.id.as_str()),
    })
}

fn print_check(check: &RuleCheck) {
    let s = Style::for_stdout();
    println!();
    if check.rules.is_empty() {
        println!(
            "  {}",
            tr!(
                "No hay reglas escritas que se apliquen aquí: nada que revisar.",
                "No written rules apply here: nothing to check."
            )
        );
        println!(
            "  {}\n",
            s.muted(tr!(
                "Anótalas en Hedy → Memoria (o aprueba las que propongan tus IAs) y tus IAs también las seguirán.",
                "Write them in Hedy → Memory (or approve the ones your AIs propose) and your AIs will follow them too."
            ))
        );
        print_left_out(&s, check);
        return;
    }
    let Some(account) = &check.account else {
        println!(
            "  {}\n",
            tr!(
                "El cambio no añade líneas que revisar.",
                "The change adds no lines to check."
            )
        );
        print_left_out(&s, check);
        return;
    };
    let (files, rules, name) = (check.files, check.rules.len(), clean(&account.name));
    println!(
        "  {}\n",
        s.muted(tr!(
            "{files} archivo(s) contra {rules} regla(s) · revisado con {name}",
            "{files} file(s) against {rules} rule(s) · checked with {name}"
        ))
    );
    if check.breaks.is_empty() {
        println!(
            "  {} {}\n",
            s.ok("✓"),
            tr!(
                "Ninguna línea nueva va contra tus reglas.",
                "No new line goes against your rules."
            )
        );
        print_left_out(&s, check);
        return;
    }
    let count = check.breaks.len();
    let head = if count == 1 {
        tr!(
            "1 línea no sigue tus reglas",
            "1 line does not follow your rules"
        )
    } else {
        tr!(
            "{count} líneas no siguen tus reglas",
            "{count} lines do not follow your rules"
        )
    };
    println!("  {} {}\n", s.warn("⚠"), s.bold(head));
    for found in &check.breaks {
        let at = format!("{}:{}", clean(&found.path), found.line);
        println!("  {}  {}", s.warn(at), clean(&found.code));
        let fix = found
            .fix
            .as_deref()
            .map(|f| format!(" → {}", clean(f)))
            .unwrap_or_default();
        println!("    {}{fix}", clean(&found.message));
        if let Some(rule) = check.rules.get(found.rule - 1) {
            let n = found.rule;
            println!(
                "    {} {}",
                s.accent(format!("[{n}]")),
                s.muted(format!("«{}» · {}", clean(&rule.text), place(rule)))
            );
        }
        println!();
    }
    let broken: std::collections::HashSet<usize> = check.breaks.iter().map(|b| b.rule).collect();
    let kept = rules - broken.len();
    if kept > 0 {
        println!(
            "  {} {}",
            s.ok("✓"),
            tr!(
                "Las otras {kept} regla(s) se respetan.",
                "The other {kept} rule(s) are followed."
            )
        );
    }
    println!(
        "  {}\n",
        s.muted(tr!(
            "Solo es un aviso: nada se bloquea.",
            "Only a warning: nothing is blocked."
        ))
    );
    print_left_out(&s, check);
}

/// What was not judged: protected files (named, never sent) and the rest of
/// a change too long for one request.
fn print_left_out(s: &Style, check: &RuleCheck) {
    if !check.protected.is_empty() {
        let list = check
            .protected
            .iter()
            .map(|p| clean(p))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {}",
            s.muted(tr!(
                "Archivos protegidos, no se enviaron (solo su nombre): {list}",
                "Protected files, not sent (only their names): {list}"
            ))
        );
    }
    if check.truncated {
        println!(
            "  {}",
            s.muted(tr!(
                "El cambio era muy largo: solo se revisó una parte. Revisa por partes (--staged, un rango de commits).",
                "The change was very long: only part of it was checked. Check it in parts (--staged, a range of commits)."
            ))
        );
    }
    if !check.protected.is_empty() || check.truncated {
        println!();
    }
}
