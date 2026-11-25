// The stops of a task that works with one AI, over its conversation (docs/decisions.md §60).

const es = {
  "stops.label": "Paradas de la tarea",
  "stops.plan": "Plan",
  "stops.ok": "Tu OK",
  "stops.implement": "Implementa",
  "stops.tests": "Tests",
  "stops.e2e": "e2e",
  "stops.review": "Revisión",
  "stops.you": "Revisas",
  "stops.state.done": "hecho",
  "stops.state.current": "ahora",
  "stops.state.pending": "pendiente",
  "stops.state.failed": "falló",
  "stops.state.skipped": "no se hizo",
};

const en: Record<keyof typeof es, string> = {
  "stops.label": "The task's stops",
  "stops.plan": "Plan",
  "stops.ok": "Your OK",
  "stops.implement": "Implements",
  "stops.tests": "Tests",
  "stops.e2e": "e2e",
  "stops.review": "Review",
  "stops.you": "You review",
  "stops.state.done": "done",
  "stops.state.current": "now",
  "stops.state.pending": "to come",
  "stops.state.failed": "failed",
  "stops.state.skipped": "skipped",
};

export default { es, en };
