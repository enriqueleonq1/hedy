// The flow on a canvas: steps as tiles, the ways forward as lines and the
// ways back as dashed loops under them. Drag the background to pan, the
// mouse wheel (or a trackpad) to move, Ctrl/⌘ + wheel to zoom; drag a step to
// move it, and a step's dot onto another step to connect them.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";

import type { Flow, FlowEdge, FlowNode, FlowNodeKind, FlowWhen, RoleDef } from "../../api/types";
import { MenuButton, type MenuItem } from "../../components/ui/Menu";
import { useT } from "../../i18n";
import { bounds, layout, TILE, type Point } from "./flowModel";
import { StepIcon, stepTint } from "./StepIcon";
import { useStepText } from "./stepText";

export type Selection = { node: string } | { edge: { from: string; when: FlowWhen } } | null;

/** How far a pointer moves before a press becomes a drag. */
const DRAG = 4;
const ZOOM_MIN = 0.35;
const ZOOM_MAX = 1.6;
/** How far under its row a way back runs. */
const LOOP_DROP = 62;

type Camera = { x: number; y: number; z: number };

type Gesture =
  | { kind: "pan"; start: Point; camera: Camera; moved: boolean }
  | { kind: "move"; node: string; start: Point; origin: Point; moved: boolean }
  | { kind: "link"; from: string; when: FlowWhen; to: Point };

export function FlowCanvas({
  flow,
  roles,
  readOnly,
  selection,
  issues,
  focus,
  onSelect,
  onMove,
  onConnect,
  addItems,
  overlay,
}: {
  flow: Flow;
  roles: RoleDef[];
  readOnly: boolean;
  selection: Selection;
  /** Steps with a problem, by id. */
  issues: Set<string>;
  /** Changes when the view should fit the flow again (another flow opened). */
  focus: string;
  onSelect: (selection: Selection) => void;
  onMove: (node: string, at: Point) => void;
  onConnect: (from: string, when: FlowWhen, to: string) => void;
  /** The add-step menu for a step inserted after `after`. */
  addItems: (after: string) => MenuItem[];
  /** Floating parts drawn over the canvas (the context strip, the toolbars). */
  overlay?: ReactNode;
}) {
  const t = useT();
  const stepText = useStepText(roles);
  const box = useRef<HTMLDivElement>(null);
  const [camera, setCamera] = useState<Camera>({ x: 48, y: 120, z: 1 });
  const [gesture, setGesture] = useState<Gesture | null>(null);
  const placedAt = useMemo(() => layout(flow), [flow]);
  // A step being dragged is drawn where the pointer is, before it is saved.
  const at = useMemo(() => {
    if (gesture?.kind !== "move" || !gesture.moved) return placedAt;
    const next = new Map(placedAt);
    next.set(gesture.node, gesture.origin);
    return next;
  }, [placedAt, gesture]);

  const fit = useCallback(() => {
    const el = box.current;
    if (!el) return;
    const b = bounds(placedAt);
    // Room for the labels under the tiles and the loops under the rows.
    const pad = { x: 96, top: 120, bottom: 150 };
    const w = el.clientWidth || 960;
    const h = el.clientHeight || 600;
    const z = Math.min(1, Math.max(ZOOM_MIN, Math.min((w - 2 * pad.x) / b.w, (h - pad.top - pad.bottom) / (b.h + LOOP_DROP))));
    setCamera({ z, x: (w - b.w * z) / 2 - b.x * z, y: pad.top + (h - pad.top - pad.bottom - (b.h + LOOP_DROP) * z) / 2 - b.y * z });
  }, [placedAt]);

  // A new flow opens fitted in view; edits never move the camera.
  const fitted = useRef<string | null>(null);
  useLayoutEffect(() => {
    if (fitted.current === focus) return;
    fitted.current = focus;
    fit();
  }, [focus, fit]);

  const world = (e: { clientX: number; clientY: number }): Point => {
    const r = box.current?.getBoundingClientRect();
    return { x: (e.clientX - (r?.left ?? 0) - camera.x) / camera.z, y: (e.clientY - (r?.top ?? 0) - camera.y) / camera.z };
  };

  const zoomBy = (factor: number, around?: Point) => {
    setCamera((c) => {
      const z = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, c.z * factor));
      const el = box.current;
      const p = around ?? { x: (el?.clientWidth ?? 0) / 2, y: (el?.clientHeight ?? 0) / 2 };
      return { z, x: p.x - ((p.x - c.x) * z) / c.z, y: p.y - ((p.y - c.y) * z) / c.z };
    });
  };

  // Wheel: pans, or zooms with Ctrl/⌘ (a trackpad's pinch comes as Ctrl).
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      if (e.ctrlKey || e.metaKey) {
        const r = el.getBoundingClientRect();
        zoomBy(Math.exp(-e.deltaY * 0.0025), { x: e.clientX - r.left, y: e.clientY - r.top });
      } else {
        setCamera((c) => ({ ...c, x: c.x - e.deltaX, y: c.y - e.deltaY }));
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  });

  const onBackgroundDown = (e: ReactPointerEvent) => {
    if (e.button !== 0 || e.target !== e.currentTarget) return;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    setGesture({ kind: "pan", start: { x: e.clientX, y: e.clientY }, camera, moved: false });
  };

  const onNodeDown = (e: ReactPointerEvent, node: FlowNode) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    e.currentTarget.setPointerCapture?.(e.pointerId);
    const origin = at.get(node.id) ?? { x: 0, y: 0 };
    setGesture({ kind: "move", node: node.id, start: { x: e.clientX, y: e.clientY }, origin, moved: false });
  };

  const onPortDown = (e: ReactPointerEvent, from: string, when: FlowWhen) => {
    if (e.button !== 0 || readOnly) return;
    e.stopPropagation();
    e.currentTarget.setPointerCapture?.(e.pointerId);
    setGesture({ kind: "link", from, when, to: world(e) });
  };

  const onPointerMove = (e: ReactPointerEvent) => {
    if (!gesture) return;
    if (gesture.kind === "pan") {
      const dx = e.clientX - gesture.start.x;
      const dy = e.clientY - gesture.start.y;
      if (!gesture.moved && Math.hypot(dx, dy) < DRAG) return;
      setCamera({ ...gesture.camera, x: gesture.camera.x + dx, y: gesture.camera.y + dy });
      if (!gesture.moved) setGesture({ ...gesture, moved: true });
    } else if (gesture.kind === "move") {
      const dx = (e.clientX - gesture.start.x) / camera.z;
      const dy = (e.clientY - gesture.start.y) / camera.z;
      if (readOnly || (!gesture.moved && Math.hypot(dx, dy) * camera.z < DRAG)) return;
      const base = placedAt.get(gesture.node) ?? { x: 0, y: 0 };
      // Snaps to a small grid so rows line up.
      const snap = (n: number) => Math.round(n / 8) * 8;
      setGesture({ ...gesture, moved: true, origin: { x: snap(base.x + dx), y: snap(base.y + dy) } });
    } else {
      setGesture({ ...gesture, to: world(e) });
    }
  };

  const onPointerUp = (e: ReactPointerEvent) => {
    const g = gesture;
    setGesture(null);
    if (!g) return;
    if (g.kind === "pan" && !g.moved) onSelect(null);
    if (g.kind === "move") {
      if (g.moved) onMove(g.node, g.origin);
      else onSelect({ node: g.node });
    }
    if (g.kind === "link") {
      const hit = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-node-id]");
      const to = hit?.dataset.nodeId;
      if (to && to !== g.from) onConnect(g.from, g.when, to);
    }
  };

  const stepName = (node: FlowNode | undefined) => (node ? stepText(node).name : "");
  const selectedNode = selection && "node" in selection ? selection.node : null;
  const selectedEdge = selection && "edge" in selection ? selection.edge : null;
  const nodeAt = (id: string) => at.get(id) ?? { x: 0, y: 0 };
  const forwardEdges = flow.edges.filter((e) => e.when !== "fail");
  const loops = flow.edges.filter((e) => e.when === "fail");
  const linkFrom = gesture?.kind === "link" ? portPoint(nodeAt(gesture.from), gesture.when) : null;

  return (
    <div
      ref={box}
      className="flow-canvas"
      data-gesture={gesture?.kind === "pan" && gesture.moved ? "pan" : undefined}
      style={{
        backgroundSize: `${20 * camera.z}px ${20 * camera.z}px`,
        backgroundPosition: `${camera.x}px ${camera.y}px`,
      }}
      onPointerDown={onBackgroundDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={() => setGesture(null)}
    >
      <div className="flow-canvas__world" style={{ transform: `translate(${camera.x}px, ${camera.y}px) scale(${camera.z})` }}>
        <svg className="flow-canvas__edges" aria-hidden="true" overflow="visible" width="1" height="1">
          <defs>
            <linearGradient id="flow-edge" x1="0" x2="1" y1="0" y2="0">
              <stop offset="0" className="flow-canvas__stop-a" />
              <stop offset="1" className="flow-canvas__stop-b" />
            </linearGradient>
          </defs>
          {forwardEdges.map((e) => (
            <EdgePath
              key={`${e.from}-${e.when}`}
              edge={e}
              d={forwardPath(nodeAt(e.from), nodeAt(e.to))}
              selected={selectedEdge?.from === e.from && selectedEdge.when === e.when}
              onSelect={() => onSelect({ edge: { from: e.from, when: e.when } })}
            />
          ))}
          {loops.map((e) => (
            <EdgePath
              key={`${e.from}-${e.when}`}
              edge={e}
              d={loopPath(nodeAt(e.from), nodeAt(e.to))}
              loop={checkOf(flow, e.from)}
              selected={selectedEdge?.from === e.from && selectedEdge.when === e.when}
              onSelect={() => onSelect({ edge: { from: e.from, when: e.when } })}
            />
          ))}
          {linkFrom && gesture?.kind === "link" && (
            <path className="flow-edge flow-edge--draft" d={freePath(linkFrom, gesture.to, gesture.when)} />
          )}
        </svg>

        {/* What each line means, and the "+" that inserts a step on it. */}
        {forwardEdges.map((e) => {
          const a = portPoint(nodeAt(e.from), e.when);
          const b = { x: nodeAt(e.to).x, y: nodeAt(e.to).y + TILE / 2 };
          const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
          const from = flow.nodes.find((n) => n.id === e.from);
          return (
            <div key={`${e.from}-${e.when}-mid`} className="flow-edge-mid" style={{ left: mid.x, top: mid.y }}>
              {e.when === "pass" && <span className="flow-edge-label">{t(from && checkOf(flow, e.from) === "verdict" ? "flows.edge.approve" : "flows.edge.pass")}</span>}
              {!readOnly && (
                <MenuButton
                  label={t("flows.add.between", { from: from ? stepText(from).name : "", to: stepName(flow.nodes.find((n) => n.id === e.to)) })}
                  className="flow-edge-add"
                  items={addItems(e.from)}
                  align="start"
                >
                  <PlusGlyph />
                </MenuButton>
              )}
            </div>
          );
        })}
        {loops.map((e) => {
          const a = nodeAt(e.from);
          const b = nodeAt(e.to);
          const y = Math.max(a.y, b.y) + TILE + LOOP_DROP;
          const check = checkOf(flow, e.from);
          return (
            <button
              key={`${e.from}-loop`}
              type="button"
              className="flow-edge-label flow-edge-label--loop"
              data-check={check}
              aria-pressed={selectedEdge?.from === e.from && selectedEdge.when === "fail"}
              style={{ left: (a.x + b.x + TILE) / 2, top: y }}
              onPointerDown={(ev) => ev.stopPropagation()}
              onClick={() => onSelect({ edge: { from: e.from, when: "fail" } })}
            >
              {t(check === "verdict" ? "flows.edge.changes" : "flows.edge.fail", { rounds: String(e.maxRounds ?? "∞") })}
            </button>
          );
        })}

        {flow.nodes.map((node) => {
          const p = nodeAt(node.id);
          const text = stepText(node);
          return (
            <div
              key={node.id}
              className="flow-node"
              data-kind={node.kind}
              data-node-id={node.id}
              data-selected={selectedNode === node.id || undefined}
              data-dragging={(gesture?.kind === "move" && gesture.node === node.id && gesture.moved) || undefined}
              style={{ left: p.x, top: p.y, ["--tint" as string]: stepTint(node, roles) }}
            >
              <button
                type="button"
                className="flow-node__tile"
                aria-label={text.name}
                aria-description={text.detail || undefined}
                aria-pressed={selectedNode === node.id}
                onPointerDown={(e) => onNodeDown(e, node)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    onSelect({ node: node.id });
                  }
                }}
              >
                <StepIcon node={node} roles={roles} />
                {issues.has(node.id) && <span className="flow-node__issue" aria-label={t("flows.issue.badge")} />}
              </button>
              {node.kind !== "start" && <span className="flow-node__port flow-node__port--in" aria-hidden="true" />}
              {node.kind !== "end" && (
                <span
                  className="flow-node__port flow-node__port--out"
                  aria-hidden="true"
                  data-when={forwardOf(node)}
                  onPointerDown={(e) => onPortDown(e, node.id, forwardOf(node))}
                />
              )}
              {node.kind === "condition" && (
                <span
                  className="flow-node__port flow-node__port--fail"
                  aria-hidden="true"
                  data-when="fail"
                  onPointerDown={(e) => onPortDown(e, node.id, "fail")}
                />
              )}
              <span className="flow-node__name">{text.name}</span>
              {text.detail && <span className="flow-node__detail">{text.detail}</span>}
            </div>
          );
        })}
      </div>

      {overlay}

      <div className="flow-zoom" role="group" aria-label={t("flows.zoom.label")} onPointerDown={(e) => e.stopPropagation()}>
        <button type="button" className="flow-zoom__btn" aria-label={t("flows.zoom.out")} onClick={() => zoomBy(1 / 1.2)}>
          <svg aria-hidden="true" width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
            <path d="M3 7h8" />
          </svg>
        </button>
        <span className="flow-zoom__value">{Math.round(camera.z * 100)} %</span>
        <button type="button" className="flow-zoom__btn" aria-label={t("flows.zoom.in")} onClick={() => zoomBy(1.2)}>
          <PlusGlyph />
        </button>
        <button type="button" className="flow-zoom__btn" aria-label={t("flows.zoom.fit")} onClick={fit}>
          <svg aria-hidden="true" width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
            <path d="M2 5V2h3M9 2h3v3M12 9v3H9M5 12H2V9" />
          </svg>
        </button>
      </div>
    </div>
  );
}

function PlusGlyph() {
  return (
    <svg aria-hidden="true" width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
      <path d="M6 2v8M2 6h8" />
    </svg>
  );
}

function EdgePath({
  edge,
  d,
  loop,
  selected,
  onSelect,
}: {
  edge: FlowEdge;
  d: string;
  loop?: string;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <g className="flow-edge-group" data-selected={selected || undefined}>
      {/* A wide invisible stroke: easy to click. */}
      <path
        className="flow-edge-hit"
        d={d}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={onSelect}
      />
      <path className={`flow-edge${edge.when === "fail" ? " flow-edge--loop" : ""}`} data-check={loop} d={d} />
    </g>
  );
}

function forwardOf(node: FlowNode): FlowWhen {
  return node.kind === "condition" ? "pass" : "after";
}

/** What a condition checks (`tests`, `rules`, `verdict`). */
function checkOf(flow: Flow, id: string): string | undefined {
  const n = flow.nodes.find((x) => x.id === id);
  return n?.kind === "condition" ? n.check : undefined;
}

/** Where a line leaves a step: its right side, or its bottom for a way back. */
function portPoint(p: Point, when: FlowWhen): Point {
  return when === "fail" ? { x: p.x + TILE / 2, y: p.y + TILE } : { x: p.x + TILE, y: p.y + TILE / 2 };
}

function forwardPath(a: Point, b: Point): string {
  const s = portPoint(a, "after");
  const e = { x: b.x, y: b.y + TILE / 2 };
  const bend = Math.max(36, Math.abs(e.x - s.x) / 2);
  return `M${s.x} ${s.y} C ${s.x + bend} ${s.y}, ${e.x - bend} ${e.y}, ${e.x} ${e.y}`;
}

/** A way back: down from the check, along under the row, up into the step. */
function loopPath(a: Point, b: Point): string {
  const s = portPoint(a, "fail");
  const e = { x: b.x + TILE / 2, y: b.y + TILE };
  const y = Math.max(a.y, b.y) + TILE + LOOP_DROP;
  const r = 18;
  const dir = e.x < s.x ? -1 : 1;
  return [
    `M${s.x} ${s.y}`,
    `L${s.x} ${y - r}`,
    `Q${s.x} ${y}, ${s.x + dir * r} ${y}`,
    `L${e.x - dir * r} ${y}`,
    `Q${e.x} ${y}, ${e.x} ${y - r}`,
    `L${e.x} ${e.y}`,
  ].join(" ");
}

/** The line being drawn from a dot to the pointer. */
function freePath(s: Point, e: Point, when: FlowWhen): string {
  if (when === "fail") return `M${s.x} ${s.y} C ${s.x} ${s.y + 60}, ${e.x} ${e.y + 60}, ${e.x} ${e.y}`;
  const bend = Math.max(36, Math.abs(e.x - s.x) / 2);
  return `M${s.x} ${s.y} C ${s.x + bend} ${s.y}, ${e.x - bend} ${e.y}, ${e.x} ${e.y}`;
}

export type { FlowNodeKind };
