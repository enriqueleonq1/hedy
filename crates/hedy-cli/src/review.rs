//! `hedy review`: the change against the written rules (docs/decisions.md
//! §37), each line that breaks one shown once, citing its notes. What it
//! finds is only a warning (exit 0), and so is a change with nothing to
//! check; an error (an account at its limit, no account) exits 1, so a git
//! hook runs it as `hedy review --staged || true`.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::Result;
use hedy_core::application::{AppError, Hedy, RuleCheck, RuleTarget};
use hedy_core::domain::rule_check::RuleBreak;
use hedy_core::domain::{Category, TaskId};
use hedy_core::tr;
use serde_json::json;

use crate::memory_ask::place;
use crate::spinner;
use crate::style::Style;
use crate::team::{clean, safe_json};

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

/// How a check ended, as the JSON says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    /// Nothing changed.
    NoChanges,
    /// No written rule applies here; nothing was sent.
    NoRules,
    /// The change adds no line to judge; nothing was sent.
    NothingToCheck,
    /// The answer said neither what breaks a rule nor that nothing does.
    Unread,
    Checked,
}

impl Status {
    fn of(check: &RuleCheck) -> Self {
        if check.rules.is_empty() {
            Self::NoRules
        } else if check.account.is_none() {
            Self::NothingToCheck
        } else if check.unread {
            Self::Unread
        } else {
            Self::Checked
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::NoChanges => "no_changes",
            Self::NoRules => "no_rules",
            Self::NothingToCheck => "nothing_to_check",
            Self::Unread => "unread",
            Self::Checked => "checked",
        }
    }
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
    let result = spinner::while_running(
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
    );
    let check = match result {
        Ok(check) => check,
        // Nothing to check is no failure: `git commit --amend` in a hook.
        Err(AppError::RulesNoChanges) => {
            if options.json {
                let value = json!({ "status": Status::NoChanges.as_str() });
                println!("{}", safe_json(&value)?);
            } else {
                println!(
                    "\n  {}\n",
                    tr!(
                        "No hay cambios que revisar.",
                        "There are no changes to check."
                    )
                );
            }
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    if options.json {
        let value = as_json(&check, options.model.as_deref());
        println!("{}", safe_json(&value)?);
    } else {
        print_check(&check);
    }
    Ok(())
}

fn as_json(check: &RuleCheck, model: Option<&str>) -> serde_json::Value {
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
        "status": Status::of(check).as_str(),
        "breaks": check.breaks,
        "rules": rules,
        "files": check.files,
        "protected": check.protected,
        "truncated": check.truncated,
        "account": check.account.as_ref().map(|a| a.id.as_str()),
        "model": model,
    })
}

fn print_check(check: &RuleCheck) {
    let s = Style::for_stdout();
    println!();
    match Status::of(check) {
        Status::NoRules => {
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
        Status::NothingToCheck => {
            println!(
                "  {}\n",
                tr!(
                    "El cambio no añade líneas que revisar.",
                    "The change adds no lines to check."
                )
            );
            print_left_out(&s, check);
            return;
        }
        _ => {}
    }
    print_summary(&s, check);
    if check.unread {
        println!(
            "  {} {}",
            s.warn("⚠"),
            tr!(
                "No se pudo leer la respuesta de la IA: el cambio no quedó revisado. Vuelve a intentarlo.",
                "The AI's answer could not be read: the change was not checked. Try again."
            )
        );
        println!();
        print_left_out(&s, check);
        return;
    }
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
    // One block per line, with every rule it breaks.
    let mut lines: BTreeMap<(&str, u32), Vec<&RuleBreak>> = BTreeMap::new();
    for found in &check.breaks {
        lines
            .entry((found.path.as_str(), found.line))
            .or_default()
            .push(found);
    }
    let head = match lines.len() {
        1 => tr!(
            "1 línea no sigue tus reglas",
            "1 line does not follow your rules"
        ),
        n => tr!(
            "{n} líneas no siguen tus reglas",
            "{n} lines do not follow your rules"
        ),
    };
    println!("  {} {}\n", s.warn("⚠"), s.bold(head));
    for ((path, line), found) in &lines {
        let at = format!("{}:{line}", clean(path));
        println!("  {}  {}", s.warn(at), clean(&found[0].code));
        for each in found {
            let fix = each
                .fix
                .as_deref()
                .map(|f| format!(" → {}", clean(f)))
                .unwrap_or_default();
            println!("    {}{fix}", clean(&each.message));
            let rule = each.rule.checked_sub(1).and_then(|i| check.rules.get(i));
            if let Some(rule) = rule {
                println!(
                    "    {} {}",
                    s.accent(format!("[{}]", each.rule)),
                    s.muted(format!("«{}» · {}", clean(&rule.text), place(rule)))
                );
            }
        }
        println!();
    }
    // Only what was judged whole: preferences are about how the AI talks.
    let broken: HashSet<usize> = check.breaks.iter().map(|b| b.rule).collect();
    let kept = check
        .rules
        .iter()
        .enumerate()
        .filter(|(i, r)| r.category != Category::Preference && !broken.contains(&(i + 1)))
        .count();
    if kept > 0 && !check.truncated {
        let line = if kept == 1 {
            tr!("La otra regla se respeta.", "The other rule is followed.")
        } else {
            tr!(
                "Las otras {kept} reglas se respetan.",
                "The other {kept} rules are followed."
            )
        };
        println!("  {} {line}", s.ok("✓"));
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

/// How much was checked, and with which account.
fn print_summary(s: &Style, check: &RuleCheck) {
    let Some(account) = &check.account else {
        return;
    };
    let name = clean(&account.name);
    let files = match check.files {
        1 => tr!("1 archivo", "1 file"),
        n => tr!("{n} archivos", "{n} files"),
    };
    let against = match check.rules.len() {
        1 => tr!("1 regla", "1 rule"),
        n => tr!("{n} reglas", "{n} rules"),
    };
    println!(
        "  {}\n",
        s.muted(tr!(
            "{files} contra {against} · revisado con {name}",
            "{files} against {against} · checked with {name}"
        ))
    );
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
