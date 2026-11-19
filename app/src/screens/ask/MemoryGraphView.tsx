// The memory graph (docs/decisions.md §36), drawn on a canvas in the style
// of Obsidian's: small dots by group, faint links, labels on the hubs. It
// settles in front of the user; an answer lights its notes, joins them to
// the question with running light and flies the camera to them. Drag to
// move, wheel to zoom, hover to see a node and its neighbours, click to open
// a note. With reduced motion it is drawn settled and still.

import { useEffect, useMemo, useRef } from "react";

import type { GraphNode, MemoryGraph, SourceRef } from "../../api/types";
import { useT } from "../../i18n";
import { useThemeTokens } from "../../theme/ThemeProvider";
import { accountPalette } from "../../theme/tokens";
import { alphaAt, buildSim, extent, nodeOf, remember, settle, settled, tick, type SimNode } from "./graphLayout";

/** Fill colours (never text): teams, then projects, by order of appearance. */
const hex = (label: string) => accountPalette.find((c) => c.label === label)?.hex ?? "#7AA2F7";
const TEAM_COLORS = [hex("color.purple"), hex("color.pink")];
const PROJECT_COLORS = [hex("color.cyan"), hex("color.blue"), hex("color.teal"), hex("color.orange")];

/** Time for the simulation in each frame while it settles. */
const FRAME_BUDGET_MS = 9;

interface Camera {
  x: number;
  y: number;
  z: number;
  tx: number;
  ty: number;
  tz: number;
}

export interface GraphProps {
  graph: MemoryGraph;
  /** The notes the answer cites, in order (their number is their place + 1). */
  cited: SourceRef[];
  /** Changes with each answer: the camera flies to its notes. */
  answer: number;
  onSelect: (node: GraphNode) => void;
}

export function MemoryGraphView({ graph, cited, answer, onSelect }: GraphProps) {
  const t = useT();
  const tokens = useThemeTokens();
  const canvas = useRef<HTMLCanvasElement>(null);
  const sim = useMemo(() => buildSim(graph), [graph]);
  const cam = useRef<Camera>({ x: 0, y: 0, z: 1, tx: 0, ty: 0, tz: 1 });
  const live = useRef({ cited, answer, onSelect, tokens, showAll: 0 });
  const showAll = useRef(0);

  useEffect(() => {
    live.current = { cited, answer, onSelect, tokens, showAll: showAll.current };
  }, [cited, answer, onSelect, tokens]);

  const colors = useMemo(() => {
    const map = new Map<string, string>();
    let teams = 0;
    let projects = 0;
    for (const n of graph.nodes) {
      if (map.has(n.group)) continue;
      if (n.group.startsWith("team:")) map.set(n.group, TEAM_COLORS[teams++ % TEAM_COLORS.length] ?? hex("color.purple"));
      else if (n.group.startsWith("project:")) map.set(n.group, PROJECT_COLORS[projects++ % PROJECT_COLORS.length] ?? hex("color.cyan"));
    }
    map.set("you", hex("color.yellow"));
    map.set("topic", hex("color.green"));
    return map;
  }, [graph]);

  useEffect(() => {
    const el = canvas.current;
    const ctx = el?.getContext("2d") ?? null;
    if (!el || !ctx) return;
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    if (reduce) {
      settle(sim);
      remember(sim);
    }
    const colorOf = (n: SimNode) =>
      n.node.kind === "person" ? (live.current.tokens.muted ?? "#9AA5CE") : (colors.get(n.node.group) ?? hex("color.blue"));

    let width = 1;
    let height = 1;
    let dpr = 1;
    const resize = () => {
      const box = el.getBoundingClientRect();
      dpr = Math.min(window.devicePixelRatio || 1, 2);
      width = Math.max(1, box.width);
      height = Math.max(1, box.height);
      el.width = Math.round(width * dpr);
      el.height = Math.round(height * dpr);
    };
    resize();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(resize);
    observer?.observe(el);

    const scale = () => (Math.min(width, height) * 0.46) / extent(sim).radius;
    let hover: SimNode | null = null;
    let drag: { x: number; y: number; cx: number; cy: number; moved: boolean } | null = null;
    let lastAnswer = -1;
    let lastShowAll = 0;
    let focusAt = 0;
    const start = performance.now();

    const point = (e: MouseEvent) => {
      const box = el.getBoundingClientRect();
      return {
        x: ((e.clientX - box.left) * width) / (box.width || width),
        y: ((e.clientY - box.top) * height) / (box.height || height),
      };
    };
    const nearest = (x: number, y: number): SimNode | null => {
      let best: SimNode | null = null;
      let bd = 110;
      for (const n of sim.nodes) {
        const d = (n.sx - x) ** 2 + (n.sy - y) ** 2;
        if (d < bd) {
          bd = d;
          best = n;
        }
      }
      return best;
    };
    const onMove = (e: MouseEvent) => {
      const p = point(e);
      const c = cam.current;
      if (drag) {
        const k = scale() * c.z;
        c.x = c.tx = drag.cx - (p.x - drag.x) / k;
        c.y = c.ty = drag.cy - (p.y - drag.y) / k;
        drag.moved ||= Math.abs(p.x - drag.x) + Math.abs(p.y - drag.y) > 3;
        return;
      }
      hover = nearest(p.x, p.y);
      el.style.cursor = hover ? "pointer" : "grab";
    };
    const onDown = (e: MouseEvent) => {
      const p = point(e);
      drag = { x: p.x, y: p.y, cx: cam.current.x, cy: cam.current.y, moved: false };
      el.style.cursor = "grabbing";
    };
    const onUp = (e: MouseEvent) => {
      if (drag && !drag.moved && e.target === el) {
        const p = point(e);
        const n = nearest(p.x, p.y);
        if (n) live.current.onSelect(n.node);
      }
      drag = null;
      el.style.cursor = "grab";
    };
    const onLeave = () => {
      hover = null;
    };
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const c = cam.current;
      c.tz = Math.min(6, Math.max(0.35, c.tz * Math.exp(-e.deltaY * 0.0015)));
    };
    el.addEventListener("mousemove", onMove);
    el.addEventListener("mousedown", onDown);
    window.addEventListener("mouseup", onUp);
    el.addEventListener("mouseleave", onLeave);
    el.addEventListener("wheel", onWheel, { passive: false });

    const pill = (x: number, y: number, w: number, h: number, r: number) => {
      ctx.beginPath();
      ctx.moveTo(x + r, y);
      ctx.arcTo(x + w, y, x + w, y + h, r);
      ctx.arcTo(x + w, y + h, x, y + h, r);
      ctx.arcTo(x, y + h, x, y, r);
      ctx.arcTo(x, y, x + w, y, r);
      ctx.closePath();
    };
    const tag = (n: SimNode, text: string, alpha: number, tk: Record<string, string>) => {
      const w = ctx.measureText(text).width + 18;
      const x = Math.min(n.sx + 12, width - w - 8);
      const y = Math.max(8, n.sy - 30);
      ctx.globalAlpha = alpha;
      ctx.fillStyle = tk.panel ?? "#16161E";
      pill(x, y, w, 24, 8);
      ctx.fill();
      ctx.strokeStyle = colorOf(n);
      ctx.lineWidth = 1;
      pill(x, y, w, 24, 8);
      ctx.stroke();
      ctx.fillStyle = tk.ink ?? "#C0CAF5";
      ctx.fillText(text, x + 9, y + 16);
    };

    let raf = 0;
    const frame = (now: number) => {
      const { tokens: tk, cited: refs, answer: asked } = live.current;
      // Settles a little each frame, within a budget, so the screen never stalls.
      if (!settled(sim)) {
        const until = performance.now() + FRAME_BUDGET_MS;
        do tick(sim);
        while (!settled(sim) && performance.now() < until);
        if (settled(sim)) remember(sim);
      }
      const ready = settled(sim) || alphaAt(sim.ticks) < 0.05;
      const c = cam.current;
      const citedNodes = refs.map((r) => nodeOf(sim, r)).filter((n): n is SimNode => n !== undefined);
      if (showAll.current !== lastShowAll) {
        lastShowAll = showAll.current;
        lastAnswer = asked;
        const e = extent(sim);
        c.tx = e.x;
        c.ty = e.y;
        c.tz = 1;
      } else if (ready && asked !== lastAnswer) {
        lastAnswer = asked;
        focusAt = now;
        if (citedNodes.length > 0) {
          const x = citedNodes.reduce((sum, n) => sum + n.x, 0) / citedNodes.length;
          const y = citedNodes.reduce((sum, n) => sum + n.y, 0) / citedNodes.length;
          c.tx = x;
          c.ty = y;
          c.tz = 1.6;
        }
      } else if (!ready) {
        const e = extent(sim);
        c.tx = e.x;
        c.ty = e.y;
      }
      const ease = reduce ? 1 : 0.07;
      c.x += (c.tx - c.x) * ease;
      c.y += (c.ty - c.y) * ease;
      c.z += (c.tz - c.z) * ease;
      const k = scale() * c.z;
      const cx = width / 2;
      const cy = height / 2;
      const drift = reduce ? 0 : 0.7;
      for (const n of sim.nodes) {
        n.sx = cx + (n.x + Math.sin(now * 0.0005 + n.phase) * drift - c.x) * k;
        n.sy = cy + (n.y + Math.cos(now * 0.00043 + n.phase) * drift - c.y) * k;
      }

      const lit = asked === lastAnswer ? citedNodes : [];
      const focus = new Set<SimNode>();
      for (const n of hover ? [hover] : lit) {
        focus.add(n);
        for (const m of n.nb) focus.add(m);
      }
      const focused = focus.size > 0;
      const glow = reduce ? 1 : Math.min(1, (now - focusAt) / 700);
      const intro = reduce ? 1 : Math.min(1, (now - start) / 900);

      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.globalAlpha = 1;
      ctx.fillStyle = tk.bg ?? "#1A1B26";
      ctx.fillRect(0, 0, width, height);

      // Links: faint, and those of the focus in its colour.
      ctx.lineWidth = Math.max(0.35, 0.5 * c.z);
      ctx.strokeStyle = tk.lineStrong ?? "#414868";
      ctx.globalAlpha = (focused ? 0.25 : 0.55) * intro;
      ctx.beginPath();
      const hot = [];
      for (const l of sim.links) {
        const on = hover ? l.s === hover || l.t === hover : lit.includes(l.s) || lit.includes(l.t);
        if (focused && on) {
          hot.push(l);
          continue;
        }
        ctx.moveTo(l.s.sx, l.s.sy);
        ctx.lineTo(l.t.sx, l.t.sy);
      }
      ctx.stroke();
      ctx.lineWidth = 1.2;
      for (const l of hot) {
        ctx.strokeStyle = colorOf(l.s.hub ? l.t : l.s);
        ctx.globalAlpha = 0.8 * glow;
        ctx.beginPath();
        ctx.moveTo(l.s.sx, l.s.sy);
        ctx.lineTo(l.t.sx, l.t.sy);
        ctx.stroke();
      }

      // The question, joined to the notes that answer it by running light.
      if (lit.length > 0 && !hover) {
        // Above and to the left of the notes it joins, clear of them.
        const qx = Math.max(60, lit.reduce((sum, n) => sum + n.sx, 0) / lit.length - 70);
        const qy = Math.max(40, Math.min(...lit.map((n) => n.sy)) - 70);
        ctx.setLineDash([3, 5]);
        ctx.lineWidth = 1.2;
        ctx.strokeStyle = tk.focus ?? "#9D85FF";
        ctx.globalAlpha = 0.6 * glow;
        for (const n of lit) {
          ctx.beginPath();
          ctx.moveTo(n.sx, n.sy);
          ctx.lineTo(qx, qy);
          ctx.stroke();
        }
        ctx.setLineDash([]);
        if (!reduce) {
          ctx.fillStyle = tk.ink ?? "#C0CAF5";
          lit.forEach((n, i) => {
            for (let p = 0; p < 3; p++) {
              const u = (now / 1500 + p / 3 + i * 0.17) % 1;
              ctx.globalAlpha = Math.sin(u * Math.PI) * glow;
              ctx.beginPath();
              ctx.arc(n.sx + (qx - n.sx) * u, n.sy + (qy - n.sy) * u, 1.8, 0, Math.PI * 2);
              ctx.fill();
            }
          });
          for (const l of hot) {
            const [from, to] = lit.includes(l.s) ? [l.t, l.s] : [l.s, l.t];
            const u = (now / 2200 + l.phase) % 1;
            ctx.globalAlpha = Math.sin(u * Math.PI) * 0.9 * glow;
            ctx.fillStyle = colorOf(to);
            ctx.beginPath();
            ctx.arc(from.sx + (to.sx - from.sx) * u, from.sy + (to.sy - from.sy) * u, 1.5, 0, Math.PI * 2);
            ctx.fill();
          }
        }
        const pulse = reduce ? 0.5 : (Math.sin(now * 0.004) + 1) / 2;
        ctx.save();
        ctx.shadowColor = tk.focus ?? "#9D85FF";
        ctx.shadowBlur = 22;
        ctx.fillStyle = tk.ink ?? "#C0CAF5";
        ctx.globalAlpha = glow;
        ctx.beginPath();
        ctx.arc(qx, qy, 5.5, 0, Math.PI * 2);
        ctx.fill();
        ctx.restore();
        ctx.strokeStyle = tk.focus ?? "#9D85FF";
        ctx.globalAlpha = 0.6 * (1 - pulse) * glow;
        ctx.lineWidth = 1.2;
        ctx.beginPath();
        ctx.arc(qx, qy, 9 + pulse * 14, 0, Math.PI * 2);
        ctx.stroke();
        ctx.globalAlpha = glow;
        ctx.font = '600 11px "JetBrains Mono Variable", ui-monospace, monospace';
        ctx.textAlign = "center";
        ctx.fillStyle = tk.ink ?? "#C0CAF5";
        ctx.fillText(t("ask.graph.question"), qx, qy - 16);
      }

      // Nodes: dots by group; the cited ones glow and pulse.
      for (const n of sim.nodes) {
        const color = colorOf(n);
        const r = n.r * (0.55 + 0.45 * c.z);
        ctx.globalAlpha = intro * (focused ? (focus.has(n) ? 1 : 0.14) : n.hub ? 1 : 0.85);
        ctx.fillStyle = color;
        if (lit.includes(n)) {
          ctx.save();
          ctx.shadowColor = color;
          ctx.shadowBlur = 18 * glow;
          ctx.beginPath();
          ctx.arc(n.sx, n.sy, r + 1.5, 0, Math.PI * 2);
          ctx.fill();
          ctx.restore();
          const ring = reduce ? 0.4 : (now / 1600 + n.phase) % 1;
          ctx.strokeStyle = color;
          ctx.globalAlpha = (1 - ring) * 0.8 * glow;
          ctx.lineWidth = 1.2;
          ctx.beginPath();
          ctx.arc(n.sx, n.sy, r + 3 + ring * 16, 0, Math.PI * 2);
          ctx.stroke();
        } else {
          ctx.beginPath();
          ctx.arc(n.sx, n.sy, r, 0, Math.PI * 2);
          ctx.fill();
        }
      }

      // Labels: the hubs (topics and people when near, or in focus), and
      // the cited or hovered note.
      ctx.textAlign = "center";
      for (const n of sim.nodes) {
        const main = n.node.kind === "team" || n.node.kind === "project" || n.node.kind === "you";
        if (!n.hub || (!main && c.z < 1.3 && !focus.has(n))) continue;
        ctx.globalAlpha = intro * (focused ? (focus.has(n) ? 0.95 : 0.12) : 0.8);
        ctx.fillStyle = main ? (tk.ink ?? "#C0CAF5") : (tk.muted ?? "#9AA5CE");
        ctx.font = `${main ? 600 : 500} 11px "Inter Variable", system-ui, sans-serif`;
        ctx.fillText(n.node.label, n.sx, n.sy + n.r * (0.55 + 0.45 * c.z) + 13);
      }
      ctx.font = '500 12px "Inter Variable", system-ui, sans-serif';
      ctx.textAlign = "left";
      const short = (text: string) => (text.length > 60 ? `${text.slice(0, 59)}…` : text);
      if (hover) {
        const text = hover.hub ? t("ask.graph.links", { name: hover.node.label, n: hover.nb.length }) : short(hover.node.label);
        tag(hover, text, 0.95, tk);
      } else {
        lit.forEach((n, i) => tag(n, `${i + 1} · ${short(n.node.label)}`, 0.95 * glow, tk));
      }
      ctx.globalAlpha = 1;
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => {
      cancelAnimationFrame(raf);
      observer?.disconnect();
      el.removeEventListener("mousemove", onMove);
      el.removeEventListener("mousedown", onDown);
      window.removeEventListener("mouseup", onUp);
      el.removeEventListener("mouseleave", onLeave);
      el.removeEventListener("wheel", onWheel);
    };
  }, [sim, colors, t]);

  const notes = graph.nodes.filter((n) => n.kind === "note").length;
  const groups = [...colors.entries()].filter(([group]) => group.startsWith("team:") || group.startsWith("project:"));
  const zoom = (factor: number) => {
    const c = cam.current;
    c.tz = Math.min(6, Math.max(0.35, c.tz * factor));
  };

  return (
    <div className="ask-graph">
      <canvas
        ref={canvas}
        className="ask-graph__canvas"
        role="img"
        aria-label={t("ask.graph.label", { notes, links: graph.links.length })}
      />
      <div className="ask-graph__head">
        <span className="ask-graph__live" aria-hidden />
        <span className="ask-graph__title">{t("ask.graph.title")}</span>
      </div>
      <div className="ask-graph__tools">
        <button type="button" aria-label={t("ask.graph.zoomIn")} title={t("ask.graph.zoomIn")} onClick={() => zoom(1.3)}>
          +
        </button>
        <button type="button" aria-label={t("ask.graph.zoomOut")} title={t("ask.graph.zoomOut")} onClick={() => zoom(1 / 1.3)}>
          −
        </button>
        <button
          type="button"
          onClick={() => {
            showAll.current++;
          }}
        >
          {t("ask.graph.all")}
        </button>
      </div>
      <ul className="ask-graph__legend" aria-label={t("ask.graph.legend")}>
        {groups.map(([group, color]) => (
          <li key={group}>
            <span className="ask-graph__dot" style={{ background: color }} aria-hidden />
            {graph.nodes.find((n) => n.group === group && n.kind !== "note")?.label ?? group}
          </li>
        ))}
        <li>
          <span className="ask-graph__dot" style={{ background: hex("color.yellow") }} aria-hidden />
          {t("ask.graph.yours")}
        </li>
        <li>
          <span className="ask-graph__dot" style={{ background: hex("color.green") }} aria-hidden />
          {t("ask.graph.topics")}
        </li>
      </ul>
      <p className="ask-graph__count">{t("ask.graph.count", { notes, links: graph.links.length })}</p>
    </div>
  );
}
