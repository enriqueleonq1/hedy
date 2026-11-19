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
  /** Which graph it is (its nodes' ids). */
  seed: number;
}

/** Ticks until the layout is settled. */
export const SETTLE_TICKS = 260;
/** How far a group of nodes must be (its size over its distance) to push as one (Barnes–Hut). */
const THETA2 = 1.44;

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
  const mix = (text: string) => {
    for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 16777619);
  };
  for (const n of graph.nodes) mix(n.id);
  for (const l of graph.links) mix(`${l.source}>${l.target}`);
  return h >>> 0;
}

/** Settled layouts, by graph: coming back to the screen shows it at once. */
const settledLayouts = new Map<number, Map<string, { x: number; y: number }>>();
/** The last settled layout: a memory that changed a little starts from it. */
let lastLayout = new Map<string, { x: number; y: number }>();
/** Ticks left to a layout that mostly starts from the last one. */
const WARM_TICKS = 70;

/** Remembers the settled layout of `sim` for the next time it is built. */
export function remember(sim: Sim): void {
  if (!settled(sim)) return;
  const layout = new Map<string, { x: number; y: number }>();
  for (const n of sim.nodes) layout.set(n.node.id, { x: n.x, y: n.y });
  if (settledLayouts.size > 8) settledLayouts.clear();
  settledLayouts.set(sim.seed, layout);
  lastLayout = layout;
}

export function buildSim(graph: MemoryGraph): Sim {
  const seed = seedOf(graph);
  const rnd = random(seed);
  const known = settledLayouts.get(seed);
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
  const ready = known !== undefined && nodes.every((n) => known.has(n.node.id));
  // The same memory: as it was. One that changed a little: from where it
  // was, new notes beside a neighbour, and only a short settle.
  const from = ready ? known : lastLayout;
  let found = 0;
  for (const n of nodes) {
    const at = from.get(n.node.id);
    if (at) {
      n.x = at.x;
      n.y = at.y;
      found++;
    }
  }
  const warm = !ready && nodes.length > 0 && found / nodes.length >= 0.8;
  if (warm) {
    for (const n of nodes) {
      const near = from.has(n.node.id) ? undefined : n.nb.find((m) => from.has(m.node.id));
      if (near) {
        n.x = near.x + (rnd() - 0.5) * 30;
        n.y = near.y + (rnd() - 0.5) * 30;
      }
    }
  }
  return { nodes, links, byId, ticks: ready ? SETTLE_TICKS : warm ? SETTLE_TICKS - WARM_TICKS : 0, seed };
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
  const tree = quadTree(nodes);
  for (const a of nodes) {
    repel(a, tree, -36 * alpha);
    a.vx -= a.x * 0.03 * alpha;
    a.vy -= a.y * 0.03 * alpha;
  }
  for (const n of nodes) {
    n.vx *= 0.6;
    n.vy *= 0.6;
    n.x += n.vx;
    n.y += n.vy;
  }
}

/** A square of the plane and what it holds: its nodes' weight and centre. */
interface Quad {
  x: number;
  y: number;
  size: number;
  weight: number;
  cx: number;
  cy: number;
  /** A leaf's nodes (more than one only when they share a point). */
  bodies: SimNode[];
  kids: Quad[] | null;
}

/** How much a node pushes: a hub, three times a note. */
const weightOf = (n: SimNode) => (n.hub ? 3 : 1);

/** The nodes in a quadtree, each square knowing its weight and centre. */
function quadTree(nodes: SimNode[]): Quad {
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const n of nodes) {
    x0 = Math.min(x0, n.x);
    y0 = Math.min(y0, n.y);
    x1 = Math.max(x1, n.x);
    y1 = Math.max(y1, n.y);
  }
  const size = Math.max(x1 - x0, y1 - y0, 1) + 1;
  const root: Quad = { x: x0, y: y0, size, weight: 0, cx: 0, cy: 0, bodies: [], kids: null };
  for (const n of nodes) insert(root, n, 0);
  return root;
}

function insert(quad: Quad, n: SimNode, depth: number): void {
  const w = weightOf(n);
  quad.cx = (quad.cx * quad.weight + n.x * w) / (quad.weight + w);
  quad.cy = (quad.cy * quad.weight + n.y * w) / (quad.weight + w);
  quad.weight += w;
  if (!quad.kids) {
    if (quad.bodies.length === 0 || depth > 24) {
      quad.bodies.push(n);
      return;
    }
    const half = quad.size / 2;
    quad.kids = [0, 1, 2, 3].map((i) => ({
      x: quad.x + (i % 2) * half,
      y: quad.y + Math.floor(i / 2) * half,
      size: half,
      weight: 0,
      cx: 0,
      cy: 0,
      bodies: [],
      kids: null,
    }));
    const moved = quad.bodies;
    quad.bodies = [];
    for (const b of moved) place(quad, b, depth);
  }
  place(quad, n, depth);
}

function place(quad: Quad, n: SimNode, depth: number): void {
  const half = quad.size / 2;
  const i = (n.x >= quad.x + half ? 1 : 0) + (n.y >= quad.y + half ? 2 : 0);
  const kid = quad.kids?.[i];
  if (kid) insert(kid, n, depth + 1);
}

/** Pushes `a` away from every other node, far groups as one. */
function repel(a: SimNode, root: Quad, strength: number): void {
  const stack: Quad[] = [root];
  for (let quad = stack.pop(); quad; quad = stack.pop()) {
    if (quad.weight === 0) continue;
    const dx = quad.cx - a.x;
    const dy = quad.cy - a.y;
    let l2 = dx * dx + dy * dy;
    if (quad.kids && (quad.size * quad.size) / Math.max(l2, 1) > THETA2) {
      for (const kid of quad.kids) stack.push(kid);
      continue;
    }
    if (quad.kids) {
      if (l2 < 1) l2 = 1;
      const w = (strength * quad.weight) / l2;
      a.vx += dx * w;
      a.vy += dy * w;
      continue;
    }
    for (const b of quad.bodies) {
      if (b === a) continue;
      const bx = b.x - a.x;
      const by = b.y - a.y;
      let d2 = bx * bx + by * by;
      if (d2 < 1) d2 = 1;
      const w = (strength * weightOf(b)) / d2;
      a.vx += bx * w;
      a.vy += by * w;
    }
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
