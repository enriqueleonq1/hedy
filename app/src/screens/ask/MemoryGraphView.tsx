// The memory graph (docs/decisions.md §36), drawn on a canvas in the style
// of Obsidian's: small dots by group, faint links, labels on the hubs. It
// settles in front of the user; an answer lights its notes, joins them to
// the question with running light and flies the camera to them. Drag to
// move, wheel to zoom, hover to see a node and its neighbours, click to open
// a note. With reduced motion it is drawn settled and still.

import { useEffect, useMemo, useRef } from "react";

import type { GraphNode, MemoryGraph, SourceRef } from "../../api/types";
import { useT } from "../../i18n";
import { useThemeName, useThemeTokens } from "../../theme/ThemeProvider";
import { accountPalette, DEFAULT_ACCOUNT_COLOR, themes } from "../../theme/tokens";
import { revealHidden } from "./citations";
import { alphaAt, buildSim, extent, nodeOf, remember, settle, settled, tick, type SimNode } from "./graphLayout";

/** Fill colours (never text): teams, then projects, by order of appearance. */
const hex = (label: string) => accountPalette.find((c) => c.label === label)?.hex ?? DEFAULT_ACCOUNT_COLOR;
/** Tokens a theme might lack (a derived one): the dark theme's. */
const FALLBACK = themes.dark;
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
  /** The notes the answer cites, with their number in it. */
  cited: { ref: SourceRef; n: number }[];
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
  const light = useThemeName() === "light";
  const live = useRef({ cited, answer, onSelect, tokens, light });
  const showAll = useRef(0);
  /** Draws again when the loop rests (reduced motion, nothing moving). */
  const wake = useRef<() => void>(() => {});

  useEffect(() => {
    live.current = { cited, answer, onSelect, tokens, light };
    wake.current();
  }, [cited, answer, onSelect, tokens, light]);

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
      n.node.kind === "person" ? (live.current.tokens.muted ?? FALLBACK.muted) : (colors.get(n.node.group) ?? hex("color.blue"));

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
    // A resize clears the canvas: draw again, even when the loop rests.
    const observer =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(() => {
            resize();
            wake.current();
          });
    observer?.observe(el);

    // Its extent changes only while it settles.
    let ext = extent(sim);
    const scale = () => (Math.min(width, height) * 0.46) / ext.radius;
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
      wakeUp();
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
      if (e.button !== 0) return;
      wakeUp();
      const p = point(e);
      drag = { x: p.x, y: p.y, cx: cam.current.x, cy: cam.current.y, moved: false };
      el.style.cursor = "grabbing";
    };
    const onUp = (e: MouseEvent) => {
      if (drag && !drag.moved && e.target === el && e.button === 0) {
        const p = point(e);
        const n = nearest(p.x, p.y);
        if (n) live.current.onSelect(n.node);
      }
      drag = null;
      el.style.cursor = "grab";
    };
    const onLeave = () => {
      hover = null;
      wakeUp();
    };
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      wakeUp();
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
      ctx.fillStyle = tk.panel ?? FALLBACK.panel;
      pill(x, y, w, 24, 8);
      ctx.fill();
      ctx.strokeStyle = colorOf(n);
      ctx.lineWidth = 1;
      pill(x, y, w, 24, 8);
      ctx.stroke();
      ctx.fillStyle = tk.ink ?? FALLBACK.ink;
      ctx.fillText(text, x + 9, y + 16);
    };

    let raf = 0;
    const wakeUp = () => {
      if (!raf) raf = requestAnimationFrame(frame);
    };
    wake.current = wakeUp;
    const short = (text: string) => {
      const shown = revealHidden(text);
      return shown.length > 60 ? `${shown.slice(0, 59)}…` : shown;
    };
    const frame = (now: number) => {
      raf = 0;
      const { tokens: tk, cited: refs, answer: asked, light: lightTheme } = live.current;
      // Settles a little each frame, within a budget, so the screen never stalls.
      if (!settled(sim)) {
        const until = performance.now() + FRAME_BUDGET_MS;
        do tick(sim);
        while (!settled(sim) && performance.now() < until);
        if (settled(sim)) remember(sim);
        ext = extent(sim);
      }
      const ready = settled(sim) || alphaAt(sim.ticks) < 0.05;
      const c = cam.current;
      const numberOf = new Map<SimNode, number>();
      for (const c of refs) {
        const node = nodeOf(sim, c.ref);
        if (node && !numberOf.has(node)) numberOf.set(node, c.n);
      }
      const citedNodes = [...numberOf.keys()];
      if (showAll.current !== lastShowAll) {
        lastShowAll = showAll.current;
        lastAnswer = asked;
        c.tx = ext.x;
        c.ty = ext.y;
        c.tz = 1;
      } else if (ready && asked !== lastAnswer) {
        lastAnswer = asked;
        focusAt = now;
        if (citedNodes.length > 0) {
          const x = citedNodes.reduce((sum, n) => sum + n.x, 0) / citedNodes.length;
          const y = citedNodes.reduce((sum, n) => sum + n.y, 0) / citedNodes.length;
          c.tx = x;
          c.ty = y;
          // Near, but with every cited note on screen.
          const far = Math.max(1, ...citedNodes.map((n) => Math.hypot(n.x - x, n.y - y)));
          c.tz = Math.max(0.35, Math.min(1.6, (0.36 * Math.min(width, height)) / (scale() * far)));
        }
      } else if (!ready) {
        c.tx = ext.x;
        c.ty = ext.y;
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
      ctx.fillStyle = tk.bg ?? FALLBACK.bg;
      ctx.fillRect(0, 0, width, height);

      // Links: faint, and those of the focus in its colour.
      ctx.lineWidth = Math.max(0.35, 0.5 * c.z);
      ctx.strokeStyle = tk.lineStrong ?? FALLBACK.lineStrong;
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
        ctx.strokeStyle = tk.focus ?? FALLBACK.focus;
        ctx.globalAlpha = 0.6 * glow;
        for (const n of lit) {
          ctx.beginPath();
          ctx.moveTo(n.sx, n.sy);
          ctx.lineTo(qx, qy);
          ctx.stroke();
        }
        ctx.setLineDash([]);
        if (!reduce) {
          ctx.fillStyle = tk.ink ?? FALLBACK.ink;
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
        ctx.shadowColor = tk.focus ?? FALLBACK.focus;
        ctx.shadowBlur = 22;
        ctx.fillStyle = tk.ink ?? FALLBACK.ink;
        ctx.globalAlpha = glow;
        ctx.beginPath();
        ctx.arc(qx, qy, 5.5, 0, Math.PI * 2);
        ctx.fill();
        ctx.restore();
        ctx.strokeStyle = tk.focus ?? FALLBACK.focus;
        ctx.globalAlpha = 0.6 * (1 - pulse) * glow;
        ctx.lineWidth = 1.2;
        ctx.beginPath();
        ctx.arc(qx, qy, 9 + pulse * 14, 0, Math.PI * 2);
        ctx.stroke();
        ctx.globalAlpha = glow;
        ctx.font = '600 11px "JetBrains Mono Variable", ui-monospace, monospace';
        ctx.textAlign = "center";
        ctx.fillStyle = tk.ink ?? FALLBACK.ink;
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
          // On a light background the palette's fills need an edge to stand out.
          if (lightTheme) {
            ctx.strokeStyle = tk.ink ?? FALLBACK.ink;
            ctx.globalAlpha *= 0.45;
            ctx.lineWidth = 0.8;
            ctx.stroke();
          }
        }
      }

      // Labels: the hubs (topics and people when near, or in focus), and
      // the cited or hovered note.
      ctx.textAlign = "center";
      for (const n of sim.nodes) {
        const main = n.node.kind === "team" || n.node.kind === "project" || n.node.kind === "you";
        if (!n.hub || (!main && c.z < 1.3 && !focus.has(n))) continue;
        ctx.globalAlpha = intro * (focused ? (focus.has(n) ? 1 : 0.12) : 1);
        ctx.fillStyle = main ? (tk.ink ?? FALLBACK.ink) : (tk.muted ?? FALLBACK.muted);
        ctx.font = `${main ? 600 : 500} 11px "Inter Variable", system-ui, sans-serif`;
        ctx.fillText(short(n.node.label), n.sx, n.sy + n.r * (0.55 + 0.45 * c.z) + 13);
      }
      ctx.font = '500 12px "Inter Variable", system-ui, sans-serif';
      ctx.textAlign = "left";
      if (hover) {
        const text = hover.hub ? t("ask.graph.links", { name: short(hover.node.label), n: hover.nb.length }) : short(hover.node.label);
        tag(hover, text, 0.95, tk);
      } else {
        for (const n of lit) tag(n, `${numberOf.get(n) ?? ""} · ${short(n.node.label)}`, 0.95 * glow, tk);
      }
      ctx.globalAlpha = 1;
      // With reduced motion, nothing moves once it is settled and the camera
      // is there: the loop rests until something changes.
      const still =
        reduce &&
        settled(sim) &&
        !drag &&
        Math.abs(c.tx - c.x) + Math.abs(c.ty - c.y) + Math.abs(c.tz - c.z) < 0.001 &&
        showAll.current === lastShowAll &&
        asked === lastAnswer;
      if (!still) raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => {
      cancelAnimationFrame(raf);
      wake.current = () => {};
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
    wake.current();
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
            wake.current();
          }}
        >
          {t("ask.graph.all")}
        </button>
      </div>
      <ul className="ask-graph__legend" aria-label={t("ask.graph.legend")}>
        {groups.map(([group, color]) => (
          <li key={group}>
            <span className="ask-graph__dot" style={{ background: color }} aria-hidden />
            {group.startsWith("team:")
              ? t("ask.graph.team", { name: revealHidden(graph.nodes.find((n) => n.group === group && n.kind === "team")?.label ?? "") })
              : revealHidden(graph.nodes.find((n) => n.group === group && n.kind === "project")?.label ?? group)}
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
      {graph.hiddenNotes > 0 && (
        <p className="ask-graph__hidden">{t("ask.graph.hidden", { n: notes, hidden: graph.hiddenNotes })}</p>
      )}
      <p className="ask-graph__count">{t("ask.graph.count", { notes, links: graph.links.length })}</p>
    </div>
  );
}
