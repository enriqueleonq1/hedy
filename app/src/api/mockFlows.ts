// Sample flows for MockApi: Hedy's templates, its roles and a light copy of
// the backend's checks (`Flow::problems`), enough for the editor to react.

import type { Flow, FlowEdge, FlowEntry, FlowIssue, FlowNode, FlowNodeKind, RoleDef, Seat } from "./types";

const role = (id: string, name: string, summary: string, extra: Partial<RoleDef> = {}): RoleDef => ({
  id,
  name,
  summary,
  instructions: `Eres el ${name.toLowerCase()}. ${summary}.`,
  carries: {},
  delivers: "text",
  can: { write: false, run: false },
  sees: { sees: "all" },
  planFirst: false,
  ...extra,
});

export const MOCK_ROLES: RoleDef[] = [
  role("architect", "Arquitecto", "Lee el proyecto y escribe el plan", { sees: { sees: "sections", sections: ["frame"] } }),
  role("programmer", "Programador", "Implementa el plan", { can: { write: true, run: true } }),
  role("tester", "Tester", "Escribe y corre los tests", { can: { write: true, run: true } }),
  role("reviewer", "Revisor", "Revisa los cambios y da un veredicto", { delivers: "verdict" }),
  role("security", "Seguridad", "Busca problemas de seguridad y privacidad", { delivers: "verdict" }),
];

/** A straight flow from a list of steps, with ways back from conditions. */
function straight(name: string, steps: FlowNodeKind[], back: Record<number, number>): Flow {
  const nodes: FlowNode[] = [{ id: "n1", kind: "start" }, ...steps.map((s, i) => ({ ...s, id: `n${i + 2}` }) as FlowNode)];
  nodes.push({ id: `n${nodes.length + 1}`, kind: "end" });
  const edges: FlowEdge[] = [];
  nodes.forEach((from, i) => {
    const n = nodes[i + 1];
    if (n) edges.push({ from: from.id, to: n.id, when: from.kind === "condition" ? "pass" : "after" });
  });
  for (const [from, to] of Object.entries(back)) edges.push({ from: `n${from}`, to: `n${to}`, when: "fail", maxRounds: 2 });
  return { name, frame: {}, nodes, edges, triggers: ["manual"] };
}

const seat = (r: string): Seat => ({ role: r });

export function mockTemplates(): FlowEntry[] {
  return [
    {
      key: "classic",
      scope: "hedy",
      summary: "Plan, tu OK, implementación, tests y revisión: lo de siempre",
      flow: straight(
        "Pipeline clásico",
        [{ kind: "role", ...seat("architect") }, { kind: "approval" }, { kind: "role", ...seat("programmer") }, { kind: "role", ...seat("reviewer") }],
        {},
      ),
    },
    {
      key: "fix-loop",
      scope: "hedy",
      summary: "Si los tests fallan o la revisión pide cambios, vuelve al programador (máx. 2)",
      flow: straight(
        "Con bucle de corrección",
        [{ kind: "role", ...seat("programmer") }, { kind: "condition", check: "tests" }, { kind: "role", ...seat("reviewer") }, { kind: "condition", check: "verdict" }],
        { 3: 2, 5: 2 },
      ),
    },
    {
      key: "double-review",
      scope: "hedy",
      summary: "Revisión y seguridad a la vez, solo leen",
      flow: straight(
        "Doble revisión",
        [{ kind: "role", ...seat("programmer") }, { kind: "parallel", roles: [seat("reviewer"), seat("security")] }, { kind: "condition", check: "verdict" }],
        { 4: 2 },
      ),
    },
  ];
}

/** The sample project's own flow, as drawn in the approved design. */
export function mockProjectFlow(): Flow {
  const flow = straight(
    "Pago con revisión",
    [
      { kind: "role", role: "architect", model: "opus" },
      { kind: "role", role: "programmer", model: "sonnet" },
      { kind: "condition", check: "tests", command: "npm test" },
      { kind: "role", role: "reviewer" },
      { kind: "condition", check: "verdict" },
      { kind: "approval" },
    ],
    { 4: 3, 6: 3 },
  );
  flow.frame = { goal: "Pasarela de pagos", rules: ["Nada de claves en el código"], files: ["src/pagos/"] };
  flow.triggers = ["manual", "taskEnd"];
  return flow;
}

/** A flow the sample team shares: a security review with its own audit command. */
export function mockTeamFlow(): Flow {
  const flow = straight(
    "Revisión de seguridad",
    [{ kind: "role", ...seat("programmer") }, { kind: "condition", check: "tests", command: "npm run audit" }, { kind: "role", ...seat("security") }, { kind: "condition", check: "verdict" }],
    { 3: 2, 5: 2 },
  );
  flow.frame = { goal: "Nada sale sin revisar la seguridad" };
  return flow;
}

/** What the user calls a step (the backend's `Node::label`). */
export function stepLabel(node: FlowNode, roles: RoleDef[]): string {
  const name = (s: Seat) => roles.find((r) => r.id === s.role)?.name ?? s.role;
  switch (node.kind) {
    case "start":
      return "Inicio";
    case "end":
      return "Final";
    case "approval":
      return "Tu aprobación";
    case "role":
      return name(node);
    case "parallel":
      return node.roles.map(name).join(" + ");
    case "condition":
      return node.check === "tests" ? "¿Pasan los tests?" : node.check === "rules" ? "¿Sigue las reglas?" : "¿Lo aprueban?";
  }
}

/** The main checks of `Flow::problems`, in Spanish like the rest of the sample data. */
export function mockProblems(flow: Flow, roles: RoleDef[]): FlowIssue[] {
  const issues: FlowIssue[] = [];
  const on = (node: FlowNode, message: (label: string) => string) => issues.push({ node: node.id, message: message(stepLabel(node, roles)) });
  if (!flow.name.trim()) issues.push({ node: null, message: "el flujo necesita un nombre de una línea (hasta 60 caracteres)" });
  const starts = flow.nodes.filter((n) => n.kind === "start").length;
  if (starts !== 1) issues.push({ node: null, message: `el flujo necesita exactamente un inicio (tiene ${starts})` });
  if (!flow.nodes.some((n) => n.kind === "end")) issues.push({ node: null, message: "el flujo no tiene final" });
  for (const node of flow.nodes) {
    const out = flow.edges.filter((e) => e.from === node.id).map((e) => e.when).sort();
    const wanted = node.kind === "end" ? [] : node.kind === "condition" ? ["fail", "pass"] : ["after"];
    if (out.join() !== wanted.join()) {
      on(node, (l) => `el paso «${l}» no tiene las conexiones que necesita (una condición: si pasa y si falla; los demás: una sola salida; el final: ninguna)`);
    }
    const seats = node.kind === "role" ? [node] : node.kind === "parallel" ? node.roles : [];
    for (const s of seats) {
      if (!roles.some((r) => r.id === s.role)) issues.push({ node: node.id, message: `no hay un rol «${s.role}» en la biblioteca` });
    }
    if (node.kind === "parallel" && node.roles.length < 2) on(node, (l) => `el grupo en paralelo «${l}» necesita al menos dos roles`);
    if (node.kind === "condition" && node.check === "verdict") {
      const feeds = flow.edges.filter((e) => e.to === node.id).map((e) => flow.nodes.find((n) => n.id === e.from));
      const verdict = (s: Seat) => roles.find((r) => r.id === s.role)?.delivers === "verdict";
      const ok = feeds.length > 0 && feeds.every((n) => (n?.kind === "role" && verdict(n)) || (n?.kind === "parallel" && n.roles.every(verdict)));
      if (!ok) on(node, (l) => `la condición «${l}» decide por un veredicto, pero el paso anterior no entrega uno`);
    }
  }
  for (const edge of flow.edges) {
    const from = flow.nodes.find((n) => n.id === edge.from);
    if (from && edge.when === "fail" && edge.maxRounds === undefined) on(from, (l) => `el bucle que pasa por «${l}» no tiene un máximo de vueltas`);
  }
  if (issues.length === 0) {
    const start = flow.nodes.find((n) => n.kind === "start");
    const seen = new Set<string>(start ? [start.id] : []);
    const queue = [...seen];
    for (let id = queue.shift(); id !== undefined; id = queue.shift()) {
      for (const e of flow.edges) {
        if (e.from !== id || seen.has(e.to)) continue;
        seen.add(e.to);
        queue.push(e.to);
      }
    }
    for (const node of flow.nodes) if (!seen.has(node.id)) on(node, (l) => `no se llega al paso «${l}» desde el inicio`);
  }
  return issues;
}
