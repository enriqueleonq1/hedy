//! Instructions Hedy gives an agent when a task moves between phases or
//! between agents. Pure text, so it is easy to read and to test.

use super::Phase;

/// For agents without a native plan mode.
pub const PLAN_FIRST: &str = "Antes de cambiar nada, propón un plan breve y numerado para este pedido. \
No modifiques archivos todavía: espera a que el plan se apruebe.";

pub const PLAN_APPROVED: &str = "Plan aprobado. Impleméntalo ahora.";

/// Answer to a planner whose plan another agent will implement.
pub const PLAN_HANDED_OFF: &str = "Plan aprobado. Otra IA lo va a implementar; no hagas nada más.";

/// For agents that have the project's memory (the `hedy-memory` tools).
pub const MEMORY_GUIDANCE: &str = "Este proyecto tiene una memoria compartida entre IAs: decisiones, convenciones y reglas. \
Antes de elegir una convención o tomar una decisión, búscala con memory_search. \
Si tomas una decisión duradera o descubres una convención que otras IAs deberían seguir, propónla con memory_propose \
(una frase; el usuario decide si se guarda). No propongas secretos ni datos personales.";

fn section(title: &str, body: &str) -> String {
    format!("## {title}\n{}\n", body.trim())
}

/// First message of a stage that starts a new agent session.
pub fn stage_prompt(
    phase: Phase,
    request: &str,
    plan: Option<&str>,
    base_commit: &str,
    previous: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str(&section("Pedido original", request));
    if let Some(plan) = plan {
        out.push_str(&section("Plan aprobado", plan));
    }
    if let Some(previous) = previous {
        out.push_str(&section("Hecho hasta ahora", previous));
    }
    let part = match phase {
            Phase::Plan => PLAN_FIRST.to_owned(),
            Phase::Implement => "Implementa el pedido siguiendo el plan. Los cambios anteriores ya están en esta carpeta.".to_owned(),
            Phase::Tests => format!(
                "Escribe o actualiza los tests para los cambios de esta rama (compara con el commit {base_commit}) y córrelos. \
                 No cambies el código de producción salvo para arreglar un error evidente."
            ),
            Phase::Review => format!(
                "Revisa los cambios de esta rama respecto al commit {base_commit} (usa git diff): errores, casos sin cubrir, \
                 tests. Responde con una lista breve y concreta. No modifiques archivos."
            ),
        };
    out.push_str(&section("Tu parte", &part));
    out
}

/// When the user moves a task to another agent (for example, at a limit).
pub fn handoff_prompt(
    request: &str,
    plan: Option<&str>,
    previous_agent: &str,
    changed_files: &[String],
) -> String {
    let files = if changed_files.is_empty() {
        "Todavía no hay cambios.".to_owned()
    } else {
        format!(
            "Archivos cambiados hasta ahora: {}.",
            changed_files.join(", ")
        )
    };
    let mut out = section("Pedido original", request);
    if let Some(plan) = plan {
        out.push_str(&section("Plan aprobado", plan));
    }
    out.push_str(&section(
        "Contexto",
        &format!(
            "{previous_agent} estaba trabajando en esta tarea y se detuvo. {files} \
             Revisa el estado actual de la carpeta y continúa donde quedó."
        ),
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_prompts_carry_the_context() {
        let p = stage_prompt(
            Phase::Tests,
            "Agrega recordatorios",
            Some("1. Hacer"),
            "abc123",
            Some("Codex implementó"),
        );
        assert!(p.contains("## Pedido original\nAgrega recordatorios"));
        assert!(p.contains("## Plan aprobado\n1. Hacer"));
        assert!(p.contains("## Hecho hasta ahora\nCodex implementó"));
        assert!(p.contains("commit abc123"));
        let review = stage_prompt(Phase::Review, "x", None, "abc", None);
        assert!(review.contains("No modifiques archivos") && !review.contains("Plan aprobado"));
    }

    #[test]
    fn handoff_lists_changes() {
        let p = handoff_prompt(
            "Agrega X",
            None,
            "Claude Opus",
            &["a.py".into(), "b.py".into()],
        );
        assert!(p.contains("Claude Opus estaba trabajando"));
        assert!(p.contains("a.py, b.py"));
        assert!(handoff_prompt("x", None, "Codex", &[]).contains("Todavía no hay cambios"));
    }
}
