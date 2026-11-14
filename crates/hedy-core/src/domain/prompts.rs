//! Instructions Hedy gives an agent when a task moves between phases or
//! between agents. Pure text, so it is easy to read and to test.

use super::{Phase, TaskMode};
use crate::tr;

/// For agents without a native plan mode.
pub fn plan_first() -> String {
    tr!(
        "Antes de cambiar nada, propón un plan breve y numerado para este pedido. No modifiques archivos todavía: espera a que el plan se apruebe.",
        "Before changing anything, propose a short numbered plan for this request. Do not modify files yet: wait for the plan to be approved."
    )
}

pub fn plan_approved() -> String {
    tr!(
        "Plan aprobado. Impleméntalo ahora.",
        "Plan approved. Implement it now."
    )
}

/// Answer to a planner whose plan another agent will implement.
pub fn plan_handed_off() -> String {
    tr!(
        "Plan aprobado. Otra IA lo va a implementar; no hagas nada más.",
        "Plan approved. Another AI will implement it; do nothing else."
    )
}

/// For agents that have the project's memory (the `hedy-memory` tools).
/// An agent that may change the project is also asked to sum up, when it
/// finishes, the lasting decisions it made (as a proposal, and only if any).
pub fn memory_guidance(phase: Phase, mode: TaskMode) -> String {
    let mut text = tr!(
        "Este proyecto tiene una memoria compartida entre IAs: decisiones, convenciones y reglas aprobadas por el usuario. Antes de elegir una convención o tomar una decisión, búscala con memory_search: devuelve un resumen por nota (id, tema y el comienzo del texto); lee una nota completa con memory_get (por id o por tema) solo si la necesitas. Si tomas una decisión duradera o descubres una convención que otras IAs deberían seguir, propónla con memory_propose (una frase; el usuario decide si se guarda). Si trata un tema que ya está en la memoria (memory_topics los lista), pasa ese mismo topic: tu propuesta será un cambio de esa nota y no una nota nueva. No propongas secretos ni datos personales.",
        "This project has a memory shared between AIs: decisions, conventions and rules approved by the user. Before choosing a convention or making a decision, look it up with memory_search: it returns a summary per note (id, topic and the start of its text); read a whole note with memory_get (by id or topic) only when you need it. If you make a lasting decision or find a convention other AIs should follow, propose it with memory_propose (one sentence; the user decides whether it is kept). If it is about a topic already in the memory (memory_topics lists them), pass that same topic: your proposal becomes a change to that note instead of a new one. Never propose secrets or personal data."
    );
    if !phase.read_only_in(mode) {
        text.push(' ');
        text.push_str(&task_summary_guidance());
    }
    text
}

/// The end-of-task summary: optional, quiet, and only ever a proposal.
fn task_summary_guidance() -> String {
    tr!(
        "Al terminar tu trabajo, antes de tu respuesta final: si tomaste decisiones duraderas que otras IAs deberían conocer y que aún no propusiste, propón una sola nota breve que las resuma con memory_propose (categoría decision). Si no decidiste nada duradero, no propongas nada ni lo menciones.",
        "When you finish your work, before your final answer: if you made lasting decisions other AIs should know about and have not proposed yet, propose one short note summing them up with memory_propose (category decision). If you decided nothing lasting, propose nothing and do not mention it."
    )
}

/// Asks the agent to answer in the user's language.
pub fn reply_language() -> String {
    tr!("Responde en español.", "Reply in English.")
}

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
    out.push_str(&section(
        &tr!("Pedido original", "Original request"),
        request,
    ));
    if let Some(plan) = plan {
        out.push_str(&section(&tr!("Plan aprobado", "Approved plan"), plan));
    }
    if let Some(previous) = previous {
        out.push_str(&section(&tr!("Hecho hasta ahora", "Done so far"), previous));
    }
    let part = match phase {
        Phase::Plan => plan_first(),
        Phase::Implement => tr!(
            "Implementa el pedido siguiendo el plan. Los cambios anteriores ya están en esta carpeta.",
            "Implement the request following the plan. Earlier changes are already in this folder."
        ),
        Phase::Tests => tr!(
            "Escribe o actualiza los tests para los cambios de esta rama (compara con el commit {base_commit}) y córrelos. No cambies el código de producción salvo para arreglar un error evidente.",
            "Write or update the tests for this branch's changes (compare with commit {base_commit}) and run them. Do not change production code except to fix an obvious bug."
        ),
        Phase::Review => tr!(
            "Revisa los cambios de esta rama respecto al commit {base_commit} (usa git diff): errores, casos sin cubrir, tests. Responde con una lista breve y concreta. No modifiques archivos.",
            "Review this branch's changes against commit {base_commit} (use git diff): bugs, uncovered cases, tests. Answer with a short, concrete list. Do not modify files."
        ),
    };
    out.push_str(&section(&tr!("Tu parte", "Your part"), &part));
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
        tr!("Todavía no hay cambios.", "There are no changes yet.")
    } else {
        let files = changed_files.join(", ");
        tr!(
            "Archivos cambiados hasta ahora: {files}.",
            "Files changed so far: {files}."
        )
    };
    let mut out = section(&tr!("Pedido original", "Original request"), request);
    if let Some(plan) = plan {
        out.push_str(&section(&tr!("Plan aprobado", "Approved plan"), plan));
    }
    out.push_str(&section(
        &tr!("Contexto", "Context"),
        &tr!(
            "{previous_agent} estaba trabajando en esta tarea y se detuvo. {files} Revisa el estado actual de la carpeta y continúa donde quedó.",
            "{previous_agent} was working on this task and stopped. {files} Check the folder's current state and pick up where it left off."
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
    fn memory_guidance_asks_for_a_summary_only_when_the_agent_writes() {
        let implement = memory_guidance(Phase::Implement, TaskMode::Project);
        assert!(implement.contains("memory_search") && implement.contains("memory_get"));
        assert!(implement.contains("memory_topics") && implement.contains("Al terminar"));
        assert!(implement.contains("no propongas nada"));
        for (phase, mode) in [
            (Phase::Review, TaskMode::Project),
            (Phase::Implement, TaskMode::Ask),
            (Phase::Implement, TaskMode::Research),
        ] {
            let read_only = memory_guidance(phase, mode);
            assert!(read_only.contains("memory_propose") && !read_only.contains("Al terminar"));
        }
        let english = crate::i18n::with_language(crate::i18n::Language::English, || {
            memory_guidance(Phase::Tests, TaskMode::Project)
        });
        assert!(english.contains("When you finish your work") && english.contains("memory_get"));
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
