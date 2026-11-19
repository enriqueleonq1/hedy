// The memory graph's layout (docs/decisions.md §36): a force simulation in
// the style of Obsidian's graph. Links pull, every node pushes the others
// away and a soft gravity holds them, so notes cluster around their project
// or team and loose notes settle in a ring outside.

import type { GraphNode, MemoryGraph, SourceRef } from "../../api/types";

export interface SimNode {
  node: GraphNode;
  x: number;
  y: number;
  vx: number;
  vy: number;
  /** Radius in world units. */
  r: number;
  hub: boolean;
  /** The nodes it links to. */
  nb: SimNode[];
  /** For the idle drift. */
  phase: number;
  /** Where it was drawn last, on screen. */
  sx: number;
  sy: number;
}

export interface SimLink {
  s: SimNode;
  t: SimNode;
  length: number;
  bias: number;
  strength: number;
  /** For the light that runs along it. */
  phase: number;
}

export interface Sim {
  nodes: SimNode[];
  links: SimLink[];
  byId: Map<string, SimNode>;
  /** Ticks run so far. */
  ticks: number;
}

/** Ticks until the layout is settled. */
export const SETTLE_TICKS = 260;
/** Past this many nodes, far pairs are left out of the repulsion. */
const FAR_NODES = 900;
const FAR_CUTOFF = 330;

const RADIUS: Record<GraphNode["kind"], number> = {
  you: 8,
  team: 9,
  project: 7,
  topic: 3.6,
  person: 4.2,
  note: 2,
};

/** A deterministic random sequence, so the same memory draws the same graph. */
function random(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 4294967296;
  };
}

function seedOf(graph: MemoryGraph): number {
  let h = 2166136261;
  for (const n of graph.nodes) for (let i = 0; i < n.id.length; i++) h = Math.imul(h ^ n.id.charCodeAt(i), 16777619);
  return h;
}

export function buildSim(graph: MemoryGraph): Sim {
  const rnd = random(seedOf(graph));
  const byId = new Map<string, SimNode>();
  const nodes: SimNode[] = graph.nodes.map((node) => {
    const angle = rnd() * Math.PI * 2;
    const distance = Math.sqrt(rnd()) * 320;
    const sim: SimNode = {
      node,
      x: Math.cos(angle) * distance,
      y: Math.sin(angle) * distance,
      vx: 0,
      vy: 0,
      r: RADIUS[node.kind] + (node.kind === "note" ? rnd() * 1.1 : 0),
      hub: node.kind !== "note",
      nb: [],
      phase: rnd() * Math.PI * 2,
      sx: 0,
      sy: 0,
    };
    byId.set(node.id, sim);
    return sim;
  });
  const links: SimLink[] = [];
  for (const l of graph.links) {
    const s = byId.get(l.source);
    const t = byId.get(l.target);
    if (!s || !t || s === t) continue;
    s.nb.push(t);
    t.nb.push(s);
    links.push({ s, t, length: s.hub && t.hub ? 110 : 18 + rnd() * 18, bias: 0, strength: 0, phase: rnd() });
  }
  for (const l of links) {
    l.bias = l.s.nb.length / (l.s.nb.length + l.t.nb.length);
    l.strength = 1 / Math.min(l.s.nb.length, l.t.nb.length);
  }
  return { nodes, links, byId, ticks: 0 };
}

/** How strongly the simulation still moves at its `ticks`-th tick (1 → 0.001). */
export function alphaAt(ticks: number): number {
  return Math.pow(0.001, Math.min(ticks, SETTLE_TICKS) / SETTLE_TICKS);
}

/** One step of the simulation. */
export function tick(sim: Sim): void {
  const alpha = alphaAt(sim.ticks);
  sim.ticks++;
  for (const l of sim.links) {
    const { s, t } = l;
    let dx = t.x + t.vx - s.x - s.vx;
    let dy = t.y + t.vy - s.y - s.vy;
    const d = Math.sqrt(dx * dx + dy * dy) || 0.001;
    const f = ((d - l.length) / d) * alpha * l.strength;
    dx *= f;
    dy *= f;
    t.vx -= dx * l.bias;
    t.vy -= dy * l.bias;
    s.vx += dx * (1 - l.bias);
    s.vy += dy * (1 - l.bias);
  }
  const nodes = sim.nodes;
  const far = nodes.length > FAR_NODES ? FAR_CUTOFF * FAR_CUTOFF : Infinity;
  nodes.forEach((a, i) => {
    for (let j = i + 1; j < nodes.length; j++) {
      const b = nodes[j];
      if (!b) continue;
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      let l2 = dx * dx + dy * dy;
      if (l2 > far) continue;
      if (l2 < 1) l2 = 1;
      const w = (-36 * alpha) / l2;
      const wa = w * (b.hub ? 3 : 1);
      const wb = w * (a.hub ? 3 : 1);
      a.vx += dx * wa;
      a.vy += dy * wa;
      b.vx -= dx * wb;
      b.vy -= dy * wb;
    }
    a.vx -= a.x * 0.03 * alpha;
    a.vy -= a.y * 0.03 * alpha;
  });
  for (const n of nodes) {
    n.vx *= 0.6;
    n.vy *= 0.6;
    n.x += n.vx;
    n.y += n.vy;
  }
}

export function settled(sim: Sim): boolean {
  return sim.ticks >= SETTLE_TICKS;
}

/** Runs the simulation to the end at once (reduced motion, tests). */
export function settle(sim: Sim): void {
  while (!settled(sim)) tick(sim);
}

/** The radius that holds nearly every node, around their centre. */
export function extent(sim: Sim): { x: number; y: number; radius: number } {
  if (sim.nodes.length === 0) return { x: 0, y: 0, radius: 1 };
  let x = 0;
  let y = 0;
  for (const n of sim.nodes) {
    x += n.x;
    y += n.y;
  }
  x /= sim.nodes.length;
  y /= sim.nodes.length;
  const radii = sim.nodes.map((n) => Math.hypot(n.x - x, n.y - y)).sort((a, b) => a - b);
  return { x, y, radius: Math.max(40, radii[Math.floor((radii.length - 1) * 0.985)] ?? 1) };
}

/** The graph node of a source of an answer, if the graph has it. */
export function nodeOf(sim: Sim, ref: SourceRef): SimNode | undefined {
  return sim.byId.get(ref.kind === "local" ? `n:${ref.id}` : `n:${ref.team}:${ref.uid}`);
}
