//! Instructions Hedy gives an agent when a task moves between phases or
//! between agents. Pure text, so it is easy to read and to test.

use super::{Phase, TaskMode};
use crate::experiments::{self, Experiment, Experiments};
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
/// It is sent with every session: `domain::context` measures it, and the
/// `trim-guidance` experiment swaps in a shorter text.
pub fn memory_guidance(phase: Phase, mode: TaskMode) -> String {
    memory_guidance_in(phase, mode, experiments::current())
}

/// [`memory_guidance`] with the experiments in `set` (to measure both texts).
pub fn memory_guidance_in(phase: Phase, mode: TaskMode, set: Experiments) -> String {
    if set.has(Experiment::TrimGuidance) {
        return trimmed_memory_guidance(phase, mode);
    }
    let mut text = tr!(
        "Este proyecto tiene una memoria compartida entre IAs: decisiones, convenciones y reglas aprobadas por el usuario. Antes de elegir una convención o tomar una decisión, búscala con memory_search: devuelve un resumen por nota (id, tema y el comienzo del texto); lee una nota completa con memory_get (por id o por tema) solo si la necesitas. Si tomas una decisión duradera o descubres una convención que otras IAs deberían seguir, propónla con memory_propose (una frase; el usuario decide si se guarda). Si trata un tema que ya está en la memoria (memory_topics los lista), pasa ese mismo topic: tu propuesta será un cambio de esa nota y no una nota nueva. No propongas secretos ni datos personales.",
        "This project has a memory shared between AIs: decisions, conventions and rules approved by the user. Before choosing a convention or making a decision, look it up with memory_search: it returns a summary per note (id, topic and the start of its text); read a whole note with memory_get (by id or topic) only when you need it. If you make a lasting decision or find a convention other AIs should follow, propose it with memory_propose (one sentence; the user decides whether it is kept). If it is about a topic already in the memory (memory_topics lists them), pass that same topic: your proposal becomes a change to that note instead of a new one. Never propose secrets or personal data."
    );
    if !phase.read_only_in(mode) {
        text.push(' ');
        text.push_str(&task_summary_guidance());
    }
    text.push(' ');
    text.push_str(&skills_guidance(phase, mode));
    text
}

/// [`memory_guidance`] followed by the team layer, when the project is a
/// team's (its notes, quoted, always in the agent's context).
pub fn memory_context(phase: Phase, mode: TaskMode, team: Option<&str>) -> String {
    let mut text = memory_guidance(phase, mode);
    if let Some(team) = team {
        text.push_str("\n\n");
        text.push_str(team);
    }
    text
}

/// The learned skills, served by the same `hedy-memory` server. An agent
/// that may change the project may propose one skill when it worked out a
/// non-obvious, reusable procedure: only a proposal, and quiet otherwise.
pub fn skills_guidance(phase: Phase, mode: TaskMode) -> String {
    let mut text = tr!(
        "El proyecto también tiene habilidades aprobadas por el usuario: procedimientos reutilizables (correr los tests e2e, publicar una versión…). Antes de averiguar cómo se hace algo así, mira skill_list (nombre y cuándo usar cada una) y lee la que sirva con skill_get. Algunas también pueden estar entre las habilidades propias de tu CLI.",
        "The project also has skills approved by the user: reusable procedures (running the e2e tests, cutting a release…). Before working out how to do something like that, check skill_list (name and when to use each) and read the one that fits with skill_get. Some may also be among your CLI's own skills."
    );
    if !phase.read_only_in(mode) {
        text.push(' ');
        text.push_str(&tr!(
            "Al terminar una tarea que salió bien y cambió el proyecto: si, y solo si, descubriste un procedimiento reutilizable que no es obvio (cómo correr algo, cómo agregar algo aquí, una lista de pasos para publicar), propón una sola habilidad con skill_propose: un nombre corto en minúsculas con guiones, una descripción de una línea que diga cuándo usarla y los pasos en Markdown. Si ya existe una con ese nombre, tu propuesta será su actualización. Nunca incluyas secretos, tokens, claves ni valores de .env (usa marcadores como $NOMBRE_VARIABLE). El usuario decide si se aprueba. Si no hay nada así, no propongas nada ni lo menciones.",
            "When a task went well and changed the project: if, and only if, you worked out a reusable procedure that is not obvious (how to run something, how to add something here, a release checklist), propose one skill with skill_propose: a short lowercase name with dashes, a one-line description saying when to use it, and the steps in Markdown. If one with that name exists, your proposal becomes its update. Never include secrets, tokens, keys or .env values (use placeholders such as $VARIABLE_NAME). The user decides whether it is approved. If there is nothing like that, propose nothing and do not mention it."
        ));
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

/// The `trim-guidance` experiment: the same rules in about half the words
/// (docs/decisions.md §33). Skills reading for everyone; the end-of-task
/// proposals (summary note, at most one skill) only for writers.
fn trimmed_memory_guidance(phase: Phase, mode: TaskMode) -> String {
    let mut text = tr!(
        "Memoria del proyecto (notas aprobadas): antes de decidir o elegir una convención, busca con memory_search (una línea por nota); memory_get (id o tema) da una entera si hace falta. Propón decisiones duraderas o convenciones con memory_propose (una frase; el usuario decide); si el tema ya existe (memory_topics), pasa ese topic para cambiar esa nota. Sin secretos ni datos personales. Habilidades (procedimientos aprobados): antes de averiguar cómo hacer algo, mira skill_list y lee la útil con skill_get.",
        "Project memory (approved notes): before deciding or picking a convention, search it with memory_search (one line per note); memory_get (id or topic) gives a whole note if needed. Propose lasting decisions or conventions with memory_propose (one sentence; the user decides); if the topic exists (memory_topics), pass that topic to change that note. No secrets or personal data. Skills (approved procedures): before working out how to do something, check skill_list and read the one that fits with skill_get."
    );
    if !phase.read_only_in(mode) {
        text.push(' ');
        text.push_str(&end_of_task_guidance());
    }
    text
}

/// What a writer proposes when it finishes: a summary note of its lasting
/// decisions and, after a task that went well, one non-obvious reusable
/// procedure as a skill. Optional, quiet, and only ever proposals.
fn end_of_task_guidance() -> String {
    tr!(
        "Al terminar, antes de tu respuesta final, solo si aplica: una nota con memory_propose (categoría decision) con las decisiones duraderas no propuestas; si la tarea salió bien y hallaste un procedimiento reutilizable no obvio, una habilidad con skill_propose (nombre-con-guiones, una línea de cuándo usarla, pasos en Markdown; un nombre existente la actualiza; sin secretos, claves ni valores de .env: usa $VARIABLE). Si no hay nada así, no propongas nada ni lo menciones.",
        "When done, before your final answer, only if it applies: one memory_propose note (category decision) with lasting decisions not yet proposed; if the task went well and you found a non-obvious reusable procedure, one skill with skill_propose (dashed-lowercase-name, one line on when to use it, steps in Markdown; an existing name updates it; no secrets, keys or .env values: use $VARIABLE). If there is nothing like that, propose nothing and do not mention it."
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

/// The turn Hedy sends when an account's limit renews and the user had
/// chosen to wait (same account, same session) with nothing else queued.
pub fn continue_after_limit() -> String {
    tr!(
        "Tu límite se renovó. Continúa con la tarea donde la dejaste.",
        "Your limit renewed. Continue the task where you left off."
    )
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
    fn skills_guidance_only_asks_writers_for_a_proposal() {
        let implement = memory_guidance(Phase::Implement, TaskMode::Project);
        assert!(implement.contains("skill_list") && implement.contains("skill_get"));
        assert!(
            implement.contains("skill_propose") && implement.contains("Nunca incluyas secretos")
        );
        let review = skills_guidance(Phase::Review, TaskMode::Project);
        assert!(review.contains("skill_get") && !review.contains("skill_propose"));
        assert!(!skills_guidance(Phase::Implement, TaskMode::Ask).contains("skill_propose"));
        let english = crate::i18n::with_language(crate::i18n::Language::English, || {
            skills_guidance(Phase::Implement, TaskMode::Project)
        });
        assert!(english.contains("if, and only if") && english.contains("$VARIABLE_NAME"));
    }

    #[test]
    fn trimmed_guidance_keeps_every_rule_in_fewer_words() {
        use crate::experiments::with_experiments;
        let trim = Experiments::NONE.with(Experiment::TrimGuidance);
        let implement = with_experiments(trim, || {
            memory_guidance(Phase::Implement, TaskMode::Project)
        });
        for part in [
            "memory_search",
            "memory_get",
            "memory_topics",
            "memory_propose",
            "el usuario decide",
            "Sin secretos",
            "skill_list",
            "skill_get",
            "skill_propose",
            "Al terminar",
            "categoría decision",
            "$VARIABLE",
            "no propongas nada ni lo menciones",
        ] {
            assert!(implement.contains(part), "{part}");
        }
        let full = memory_guidance(Phase::Implement, TaskMode::Project);
        assert!(implement.chars().count() * 2 <= full.chars().count());
        for (phase, mode) in [
            (Phase::Review, TaskMode::Project),
            (Phase::Implement, TaskMode::Ask),
            (Phase::Implement, TaskMode::Research),
        ] {
            let read_only = with_experiments(trim, || memory_guidance(phase, mode));
            assert!(read_only.contains("memory_propose") && read_only.contains("skill_get"));
            assert!(!read_only.contains("Al terminar") && !read_only.contains("skill_propose"));
        }
        let english = with_experiments(trim, || {
            crate::i18n::with_language(crate::i18n::Language::English, || {
                memory_guidance(Phase::Tests, TaskMode::Project)
            })
        });
        assert!(english.contains("When done") && english.contains("No secrets or personal data"));
        assert!(
            english.contains("non-obvious reusable procedure") && english.contains("$VARIABLE")
        );
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
