// The flow editor's model: pure functions over a `Flow`, so every change is
// one new value (undo keeps the old ones) and the canvas only draws.

import type { Flow, FlowEdge, FlowNode, FlowNodeKind, FlowWhen } from "../../api/types";

/** A step's tile, and the room between columns and rows of the layout. */
export const TILE = 64;
export const COL = 176;
export const ROW = 148;
/** What the backend allows (`domain::flow`). */
export const MAX_ROUNDS = 10;
export const DEFAULT_ROUNDS = 2;
export const MAX_NODES = 40;
export const MAX_NAME = 60;

export type Point = { x: number; y: number };

/** A new flow: start, a programmer, the end. */
export function blankFlow(name: string): Flow {
  return {
    name,
    frame: {},
    nodes: [
      { id: "n1", kind: "start", at: { x: 0, y: 0 } },
      { id: "n2", kind: "role", role: "programmer", at: { x: COL, y: 0 } },
      { id: "n3", kind: "end", at: { x: 2 * COL, y: 0 } },
    ],
    edges: [
      { from: "n1", to: "n2", when: "after" },
      { from: "n2", to: "n3", when: "after" },
    ],
    triggers: ["manual"],
  };
}

/** The way forward out of a step: `pass` for a condition, `after` otherwise. */
export function forward(node: FlowNode): FlowWhen {
  return node.kind === "condition" ? "pass" : "after";
}

export function nodeById(flow: Flow, id: string): FlowNode | undefined {
  return flow.nodes.find((n) => n.id === id);
}

export function edgeFrom(flow: Flow, from: string, when: FlowWhen): FlowEdge | undefined {
  return flow.edges.find((e) => e.from === from && e.when === when);
}

/**
 * Where each step is drawn: its own place, or a column by its distance from
 * the start along the ways forward (ways back never push a step right), and
 * a row within the column.
 */
export function layout(flow: Flow): Map<string, Point> {
  const depth = new Map<string, number>();
  const start = flow.nodes.find((n) => n.kind === "start") ?? flow.nodes[0];
  if (start) {
    depth.set(start.id, 0);
    // Longest path over forward edges; a bound keeps a bad cycle from spinning.
    for (let round = 0; round < flow.nodes.length; round++) {
      let changed = false;
      for (const e of flow.edges) {
        if (e.when === "fail") continue;
        const d = depth.get(e.from);
        if (d === undefined) continue;
        if ((depth.get(e.to) ?? -1) < d + 1 && d + 1 <= flow.nodes.length) {
          depth.set(e.to, d + 1);
          changed = true;
        }
      }
      if (!changed) break;
    }
  }
  // Steps nothing reaches go after the rest.
  let last = Math.max(0, ...depth.values());
  for (const n of flow.nodes) if (!depth.has(n.id)) depth.set(n.id, ++last);
  const rows = new Map<number, number>();
  const at = new Map<string, Point>();
  for (const n of flow.nodes) {
    const col = depth.get(n.id) ?? 0;
    const row = rows.get(col) ?? 0;
    rows.set(col, row + 1);
    at.set(n.id, n.at ?? { x: col * COL, y: row * ROW });
  }
  return at;
}

/** The same flow with every step at its drawn place, so moving one keeps the rest. */
export function placed(flow: Flow): Flow {
  if (flow.nodes.every((n) => n.at)) return flow;
  const at = layout(flow);
  return { ...flow, nodes: flow.nodes.map((n) => ({ ...n, at: at.get(n.id) })) };
}

/** The next free id (`n7`). */
export function nextId(flow: Flow): string {
  let max = 0;
  for (const n of flow.nodes) {
    const m = /^n(\d+)$/.exec(n.id);
    if (m) max = Math.max(max, Number(m[1]));
  }
  return `n${max + 1}`;
}

/** The closest step before `id` with a role (where a failed check goes back to). */
export function roleBefore(flow: Flow, id: string): string | undefined {
  const seen = new Set<string>([id]);
  let frontier = [id];
  while (frontier.length) {
    const next: string[] = [];
    for (const to of frontier) {
      for (const e of flow.edges) {
        if (e.to !== to || e.when === "fail" || seen.has(e.from)) continue;
        seen.add(e.from);
        const from = nodeById(flow, e.from);
        if (from?.kind === "role" || from?.kind === "parallel") return e.from;
        next.push(e.from);
      }
    }
    frontier = next;
  }
  return undefined;
}

/**
 * Inserts a step on the way forward out of `after` (or on the edge `on`):
 * what came next now comes after the new step. A condition also gets its way
 * back, to the closest role before it, with the usual rounds.
 */
export function insertStep(flow: Flow, after: string, kind: FlowNodeKind): { flow: Flow; id: string } {
  const f = placed(flow);
  const from = nodeById(f, after);
  if (!from) return { flow, id: after };
  const id = nextId(f);
  const out = edgeFrom(f, after, forward(from));
  const origin = from.at ?? { x: 0, y: 0 };
  const target = out ? nodeById(f, out.to) : undefined;
  // Room for it: what is right of the step (on its row band) moves a column.
  const shift = (n: FlowNode): FlowNode =>
    n.at && n.at.x > origin.x && Math.abs(n.at.y - origin.y) < ROW * 2 ? { ...n, at: { x: n.at.x + COL, y: n.at.y } } : n;
  const node = { ...kind, id, at: { x: origin.x + COL, y: target?.at?.y ?? origin.y } } as FlowNode;
  const edges = f.edges.filter((e) => e !== out);
  edges.push({ from: after, to: id, when: forward(from) });
  if (out) edges.push({ from: id, to: out.to, when: forward(node) });
  let next: Flow = { ...f, nodes: [...f.nodes.map(shift), node], edges };
  if (node.kind === "condition") {
    const back = roleBefore(next, id);
    if (back) next = { ...next, edges: [...next.edges, { from: id, to: back, when: "fail", maxRounds: DEFAULT_ROUNDS }] };
  }
  return { flow: next, id };
}

/**
 * Takes a step out and joins what came before it to what came after; the ways
 * back that pointed at it go too (the check then asks for a new one). The
 * start and the end stay.
 */
export function removeStep(flow: Flow, id: string): Flow {
  const node = nodeById(flow, id);
  if (!node || node.kind === "start" || node.kind === "end") return flow;
  const out = edgeFrom(flow, id, forward(node));
  const edges: FlowEdge[] = [];
  for (const e of flow.edges) {
    if (e.from === id) continue;
    if (e.to !== id) edges.push(e);
    else if (e.when !== "fail" && out && out.to !== e.from) edges.push({ ...e, to: out.to });
  }
  return { ...flow, nodes: flow.nodes.filter((n) => n.id !== id), edges };
}

/** `from` goes to `to` when `when`, in place of where it went before. */
export function connect(flow: Flow, from: string, when: FlowWhen, to: string): Flow {
  if (from === to) return flow;
  const before = edgeFrom(flow, from, when);
  const edges = flow.edges.filter((e) => !(e.from === from && e.when === when));
  const edge: FlowEdge = { from, to, when };
  if (when === "fail") edge.maxRounds = before?.maxRounds ?? DEFAULT_ROUNDS;
  return { ...flow, edges: [...edges, edge] };
}

export function disconnect(flow: Flow, from: string, when: FlowWhen): Flow {
  return { ...flow, edges: flow.edges.filter((e) => !(e.from === from && e.when === when)) };
}

export function setRounds(flow: Flow, from: string, when: FlowWhen, rounds: number): Flow {
  const max = Math.min(MAX_ROUNDS, Math.max(1, Math.round(rounds) || 1));
  return { ...flow, edges: flow.edges.map((e) => (e.from === from && e.when === when ? { ...e, maxRounds: max } : e)) };
}

/** Replaces what a step is (its kind and settings), keeping its id and place. */
export function setStep(flow: Flow, id: string, kind: FlowNodeKind): Flow {
  return {
    ...flow,
    nodes: flow.nodes.map((n) => (n.id === id ? ({ ...kind, id, at: n.at } as FlowNode) : n)),
  };
}

export function moveStep(flow: Flow, id: string, at: Point): Flow {
  const f = placed(flow);
  return { ...f, nodes: f.nodes.map((n) => (n.id === id ? { ...n, at: { x: Math.round(at.x), y: Math.round(at.y) } } : n)) };
}

/** Steps in the order they run: forward from the start, then the rest. */
export function runOrder(flow: Flow): FlowNode[] {
  const at = layout({ ...flow, nodes: flow.nodes.map((n) => ({ ...n, at: undefined })) });
  return [...flow.nodes].sort((a, b) => (at.get(a.id)?.x ?? 0) - (at.get(b.id)?.x ?? 0) || (at.get(a.id)?.y ?? 0) - (at.get(b.id)?.y ?? 0));
}

/** The flow's bounds, for fitting it in view. */
export function bounds(at: Map<string, Point>): { x: number; y: number; w: number; h: number } {
  const ps = [...at.values()];
  if (!ps.length) return { x: 0, y: 0, w: TILE, h: TILE };
  const x = Math.min(...ps.map((p) => p.x));
  const y = Math.min(...ps.map((p) => p.y));
  return { x, y, w: Math.max(...ps.map((p) => p.x)) - x + TILE, h: Math.max(...ps.map((p) => p.y)) - y + TILE };
}

/** An undo history of flows: `push` after each change, one step per gesture. */
export interface History {
  past: Flow[];
  now: Flow;
  future: Flow[];
}

const KEEP = 100;

export const history = {
  start: (flow: Flow): History => ({ past: [], now: flow, future: [] }),
  push: (h: History, flow: Flow): History => (flow === h.now ? h : { past: [...h.past, h.now].slice(-KEEP), now: flow, future: [] }),
  undo: (h: History): History => {
    const prev = h.past[h.past.length - 1];
    return prev ? { past: h.past.slice(0, -1), now: prev, future: [h.now, ...h.future] } : h;
  },
  redo: (h: History): History => {
    const [next, ...rest] = h.future;
    return next ? { past: [...h.past, h.now], now: next, future: rest } : h;
  },
};
