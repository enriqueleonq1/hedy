// A project's flows (the orchestrator, docs/decisions.md §38–§45): pick one,
// see it on a canvas or as a list, change it, save it and run it. Hedy's
// templates are read-only: duplicating one makes it the user's.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useState } from "react";

import { errorMessage, useApi } from "../../api/context";
import type { Flow, FlowEntry, FlowNodeKind, Overview, RoleDef } from "../../api/types";
import { useUnsavedChanges } from "../../code/editor/unsaved";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { Segmented } from "../../components/ui/controls";
import { Dialog } from "../../components/ui/Dialog";
import type { View } from "../../components/Layout";
import { MenuButton, type MenuItem } from "../../components/ui/Menu";
import { Select, type SelectSection } from "../../components/ui/Select";
import { useToast } from "../../components/ui/Toast";
import { useT } from "../../i18n";
import { ProjectHead, projectTabView } from "../workspace/ProjectHome";
import { FlowCanvas, type Selection } from "./FlowCanvas";
import { FlowInspector } from "./FlowInspector";
import { FlowList } from "./FlowList";
import { blankFlow, connect, history, insertStep, MAX_NODES, moveStep, nodeById, removeStep, type History } from "./flowModel";
import "../../styles/flows.css";

type Mode = "canvas" | "list";

/** What is open: a saved flow (`key`) or a new one not saved yet (`key: null`). */
interface Draft {
  key: string | null;
  /** The saved version, to tell whether there are changes. */
  saved: Flow | null;
  history: History;
  /** The field typed in last: typing on in it stays one undo step. */
  merge?: string;
}

const same = (a: Flow | null, b: Flow) => a !== null && JSON.stringify(a) === JSON.stringify(b);

export function FlowsScreen({
  overview,
  project: path,
  flow: wanted,
  onNavigate,
}: {
  overview: Overview;
  project: string;
  /** The flow to open (its key); none: the project's first, else the first template. */
  flow?: string;
  onNavigate: (view: View, options?: { replace?: boolean }) => void;
}) {
  const t = useT();
  const api = useApi();
  const toast = useToast();
  const client = useQueryClient();
  const unsaved = useUnsavedChanges();
  const project = overview.projects.find((p) => p.path === path);
  const flows = useQuery({ queryKey: ["flows", path], queryFn: () => api.flows(path) });
  const rolesQuery = useQuery({ queryKey: ["flowRoles", path], queryFn: () => api.flowRoles(path) });
  const roles: RoleDef[] = useMemo(() => (rolesQuery.data ?? []).map((r) => r.role), [rolesQuery.data]);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [selection, setSelection] = useState<Selection>(null);
  const [mode, setMode] = useState<Mode>("canvas");
  const [running, setRunning] = useState(false);
  const [saving, setSaving] = useState(false);

  const entries = flows.data ?? [];
  const entry = draft?.key ? entries.find((e) => e.key === draft.key) : undefined;
  const readOnly = entry?.scope === "hedy";
  const flow = draft?.history.now ?? null;
  const dirty = !!flow && !readOnly && !same(draft?.saved ?? null, flow);

  const open = (e: FlowEntry) => {
    setDraft({ key: e.key, saved: e.flow, history: history.start(e.flow) });
    setSelection(null);
  };
  // Opens the flow asked for (or the default) once the list is there: on
  // first load, and when the address names another saved flow.
  if (flows.data && (!draft || (wanted && draft.key !== null && draft.key !== wanted))) {
    const pick = flows.data.find((e) => e.key === wanted) ?? flows.data.find((e) => e.scope !== "hedy") ?? flows.data[0];
    if (pick && pick.key !== draft?.key) open(pick);
  }

  // Live check, a moment after the last change.
  const [checked, setChecked] = useState<Flow | null>(null);
  useEffect(() => {
    const timer = setTimeout(() => setChecked(flow), 200);
    return () => clearTimeout(timer);
  }, [flow]);
  const problems = useQuery({
    queryKey: ["flowProblems", path, checked && JSON.stringify(checked)],
    queryFn: () => (checked ? api.flowProblems(path, checked) : []),
    enabled: !!checked && !readOnly,
    placeholderData: (previous) => previous,
  });
  const issues = useMemo(() => (readOnly ? [] : (problems.data ?? [])), [readOnly, problems.data]);
  const issueNodes = useMemo(() => new Set(issues.flatMap((i) => (i.node ? [i.node] : []))), [issues]);

  const change = useCallback(
    (next: Flow, merge?: string) => {
      if (readOnly) return;
      setDraft((d) => {
        if (!d) return d;
        const h = merge && merge === d.merge ? { ...d.history, now: next } : history.push(d.history, next);
        return { ...d, history: h, merge };
      });
    },
    [readOnly],
  );
  const step = (move: (h: History) => History) => setDraft((d) => (d ? { ...d, history: move(d.history), merge: undefined } : d));

  const save = useCallback(async (): Promise<boolean> => {
    if (!draft || !flow) return false;
    setSaving(true);
    try {
      const saved = await api.saveFlow(path, draft.key, flow);
      setDraft((d) => (d ? { ...d, key: saved.key, saved: saved.flow } : d));
      // Saved: the address may change now without asking about it.
      unsaved.set("flows", null);
      await client.invalidateQueries({ queryKey: ["flows", path] });
      onNavigate({ screen: "flows", project: path, flow: saved.key }, { replace: true });
      return true;
    } catch (error) {
      toast.error(errorMessage(error, t));
      return false;
    } finally {
      setSaving(false);
    }
  }, [api, client, draft, flow, onNavigate, path, t, toast, unsaved]);

  // Leaving with changes asks first (Guardar / Descartar / Cancelar).
  useEffect(() => {
    unsaved.set(
      "flows",
      dirty && flow
        ? {
            name: flow.name || t("flows.untitled"),
            save,
            discard: () => setDraft((d) => (d?.saved ? { ...d, history: history.start(d.saved) } : null)),
          }
        : null,
    );
  }, [dirty, flow, save, t, unsaved]);
  useEffect(() => () => unsaved.set("flows", null), [unsaved]);

  const switchTo = async (key: string) => {
    if (key === draft?.key) return;
    if (!(await unsaved.confirmLeave())) return;
    const e = entries.find((x) => x.key === key);
    if (!e) return;
    open(e);
    onNavigate({ screen: "flows", project: path, flow: key }, { replace: true });
  };

  const startNew = async (from?: Flow) => {
    if (!(await unsaved.confirmLeave())) return;
    const flow = from ? { ...structuredClone(from), name: t("flows.copyName", { name: from.name }) } : blankFlow(t("flows.newName"));
    setDraft({ key: null, saved: null, history: history.start(flow) });
    setSelection(null);
  };

  const remove = async () => {
    if (!draft?.key) return;
    try {
      await api.deleteFlow(path, draft.key);
      unsaved.set("flows", null);
      setDraft(null);
      await client.invalidateQueries({ queryKey: ["flows", path] });
      onNavigate({ screen: "flows", project: path }, { replace: true });
    } catch (error) {
      toast.error(errorMessage(error, t));
    }
  };

  const insertAfter = (after: string, kind: FlowNodeKind) => {
    if (!flow) return;
    const { flow: next, id } = insertStep(flow, after, kind);
    change(next);
    setSelection({ node: id });
    setMode("canvas");
  };

  /** Where "Agregar paso" puts the new step: after the selected one, or before the end. */
  const insertionPoint = (): string | undefined => {
    if (!flow) return undefined;
    const chosen = selection && "node" in selection ? nodeById(flow, selection.node) : undefined;
    if (chosen && chosen.kind !== "end") return chosen.id;
    const end = flow.nodes.find((n) => n.kind === "end");
    return flow.edges.find((e) => e.to === end?.id && e.when !== "fail")?.from ?? flow.nodes.find((n) => n.kind === "start")?.id;
  };

  const addItems = (after: string): MenuItem[] => {
    const full = (flow?.nodes.length ?? 0) >= MAX_NODES;
    const readers = roles.filter((r) => !r.can.write);
    const parallel = readers.length >= 2 ? readers.slice(0, 2) : [];
    return [
      ...roles.map((r) => ({
        label: r.name,
        hint: r.summary,
        disabled: full,
        onSelect: () => insertAfter(after, { kind: "role", role: r.id }),
      })),
      {
        label: t("flows.step.tests"),
        hint: t("flows.check.testsDetail"),
        separatorBefore: true,
        disabled: full,
        onSelect: () => insertAfter(after, { kind: "condition", check: "tests" }),
      },
      { label: t("flows.step.rules"), hint: t("flows.check.rulesDetail"), disabled: full, onSelect: () => insertAfter(after, { kind: "condition", check: "rules" }) },
      { label: t("flows.step.verdict"), hint: t("flows.check.verdictDetail"), disabled: full, onSelect: () => insertAfter(after, { kind: "condition", check: "verdict" }) },
      {
        label: t("flows.step.approval"),
        hint: t("flows.add.approvalHint"),
        separatorBefore: true,
        disabled: full,
        onSelect: () => insertAfter(after, { kind: "approval" }),
      },
      ...(parallel.length === 2
        ? [
            {
              label: t("flows.add.parallel"),
              hint: t("flows.add.parallelHint"),
              disabled: full,
              onSelect: () => insertAfter(after, { kind: "parallel", roles: parallel.map((r) => ({ role: r.id })) }),
            },
          ]
        : []),
    ];
  };

  // Keys: ⌘/Ctrl+Z, ⇧⌘Z / Ctrl+Y, Supr/⌫ removes the selection, Esc clears it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest("input, textarea, [contenteditable], [role=dialog], [role=menu], [role=listbox]")) return;
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key.toLowerCase() === "z") {
        e.preventDefault();
        step(e.shiftKey ? history.redo : history.undo);
      } else if (mod && e.key.toLowerCase() === "y") {
        e.preventDefault();
        step(history.redo);
      } else if ((e.key === "Delete" || e.key === "Backspace") && flow && !readOnly && selection) {
        e.preventDefault();
        if ("node" in selection) change(removeStep(flow, selection.node));
        else change({ ...flow, edges: flow.edges.filter((x) => !(x.from === selection.edge.from && x.when === selection.edge.when)) });
        setSelection(null);
      } else if (e.key === "Escape" && selection) {
        setSelection(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!project) return null;

  const sections: SelectSection[] = [
    { title: t("flows.picker.templates"), options: entries.filter((e) => e.scope === "hedy").map((e) => ({ value: e.key, label: e.flow.name, detail: e.summary })) },
    { title: t("flows.picker.project"), options: entries.filter((e) => e.scope === "project").map((e) => ({ value: e.key, label: e.flow.name })) },
    { title: t("flows.picker.global"), options: entries.filter((e) => e.scope === "global").map((e) => ({ value: e.key, label: e.flow.name })) },
  ].filter((s) => s.options.length > 0);
  if (draft && draft.key === null && flow) sections.unshift({ title: t("flows.picker.unsaved"), options: [{ value: "", label: flow.name || t("flows.untitled") }] });

  const status = readOnly
    ? t("flows.status.template")
    : saving
      ? t("flows.status.saving")
      : dirty
        ? t("flows.status.unsaved")
        : t("flows.status.saved");
  const canRun = !!draft?.key && !dirty && issues.length === 0;
  const point = insertionPoint();

  return (
    <div className="screen flows">
      <ProjectHead name={project.name} path={path} current="flows" onTab={(tab) => onNavigate(projectTabView(tab, path))} />
      <div className="flows-bar">
        <div className="flows-bar__group">
          <Select
            label={t("flows.picker.label")}
            title={t("flows.picker.label")}
            value={draft?.key ?? ""}
            sections={sections}
            onChange={(key) => key && void switchTo(key)}
            className="flows-bar__picker"
          />
          <Button small variant="ghost" onClick={() => void startNew()}>
            {t("flows.new")}
          </Button>
          <Segmented
            label={t("flows.mode.label")}
            value={mode}
            onChange={setMode}
            options={[
              { value: "canvas", label: t("flows.mode.canvas") },
              { value: "list", label: t("flows.mode.list") },
            ]}
          />
        </div>
        <div className="flows-bar__group">
          <span className="flows-bar__status" role="status" data-dirty={dirty || undefined}>
            {status}
          </span>
          {issues.length > 0 && (
            <button type="button" className="flows-bar__issues" onClick={() => setSelection(null)}>
              {t("flows.issues.count", { count: String(issues.length) })}
            </button>
          )}
          {!readOnly && draft?.key && (
            <ConfirmButton small variant="ghost" confirmLabel={t("flows.delete.confirm")} onConfirm={() => void remove()}>
              {t("flows.delete")}
            </ConfirmButton>
          )}
          {readOnly ? (
            flow && (
              <Button small onClick={() => void startNew(flow)}>
                {t("flows.duplicate")}
              </Button>
            )
          ) : (
            <Button small variant={dirty ? "primary" : "secondary"} disabled={!dirty || saving || issues.length > 0} onClick={() => void save()}>
              {t("flows.save")}
            </Button>
          )}
          <Button
            small
            variant={dirty ? "secondary" : "primary"}
            disabled={!canRun}
            title={canRun ? undefined : t(dirty || !draft?.key ? "flows.run.saveFirst" : "flows.run.fixFirst")}
            onClick={() => setRunning(true)}
          >
            {t("flows.run")}
          </Button>
        </div>
      </div>

      {!flow ? (
        <div className="flows-loading" aria-busy="true" />
      ) : mode === "list" ? (
        <FlowList
          flow={flow}
          roles={roles}
          issues={issueNodes}
          onOpen={(node) => {
            setSelection({ node });
            setMode("canvas");
          }}
        />
      ) : (
        <div className="flows-stage">
          <FlowCanvas
            flow={flow}
            roles={roles}
            readOnly={readOnly}
            selection={selection}
            issues={issueNodes}
            focus={`${draft?.key ?? "new"}`}
            onSelect={setSelection}
            onMove={(node, at) => change(moveStep(flow, node, at))}
            onConnect={(from, when, to) => change(connect(flow, from, when, to))}
            addItems={addItems}
            overlay={
              <>
                <FrameStrip flow={flow} onOpen={() => setSelection(null)} />
                {readOnly && (
                  <div className="flow-banner" onPointerDown={(e) => e.stopPropagation()}>
                    <span>{t("flows.template.banner")}</span>
                    <Button small variant="primary" onClick={() => void startNew(flow)}>
                      {t("flows.duplicate")}
                    </Button>
                  </div>
                )}
                {!readOnly && point && (
                  <div className="flow-dock" onPointerDown={(e) => e.stopPropagation()}>
                    <MenuButton label={t("flows.add")} className="btn btn--primary btn--small flow-dock__add" items={addItems(point)} align="start">
                      <span aria-hidden="true">+</span> {t("flows.add")}
                    </MenuButton>
                    <span className="flow-dock__hint">{t("flows.dock.hint")}</span>
                  </div>
                )}
              </>
            }
          />
          <FlowInspector
            flow={flow}
            selection={selection}
            roles={roles}
            accounts={overview.accounts}
            issues={issues}
            readOnly={readOnly}
            onChange={change}
            onSelect={setSelection}
            onRemove={(node) => {
              change(removeStep(flow, node));
              setSelection(null);
            }}
          />
        </div>
      )}

      {running && draft?.key && flow && (
        <RunDialog
          flowName={flow.name}
          flowKey={draft.key}
          onClose={() => setRunning(false)}
          onRun={async (prompt, key) => {
            try {
              const task = await api.startTask({ project: path, prompt, mode: "project", model: null, planFirst: false, flow: key });
              setRunning(false);
              onNavigate({ screen: "workspace", project: path, task: task.id });
            } catch (error) {
              toast.error(errorMessage(error, t));
            }
          }}
        />
      )}
    </div>
  );
}

/** What every role shares, over the canvas; a click opens it in the panel. */
function FrameStrip({ flow, onOpen }: { flow: Flow; onOpen: () => void }) {
  const t = useT();
  const { goal, rules = [], files = [] } = flow.frame;
  return (
    <button type="button" className="flow-frame" onPointerDown={(e) => e.stopPropagation()} onClick={onOpen}>
      <span className="flow-frame__title">{goal?.trim() ? goal : t("flows.frame.none")}</span>
      {rules.length > 0 && <span className="flow-frame__chip">{t("flows.frame.rulesCount", { count: String(rules.length) })}</span>}
      {files.slice(0, 2).map((f) => (
        <span key={f} className="flow-frame__chip flow-frame__chip--mono">
          {f}
        </span>
      ))}
      {files.length > 2 && <span className="flow-frame__chip">+{files.length - 2}</span>}
    </button>
  );
}

function RunDialog({
  flowName,
  flowKey,
  onClose,
  onRun,
}: {
  flowName: string;
  flowKey: string;
  onClose: () => void;
  onRun: (prompt: string, key: string) => Promise<void>;
}) {
  const t = useT();
  const [prompt, setPrompt] = useState("");
  const [busy, setBusy] = useState(false);
  const go = async () => {
    if (!prompt.trim() || busy) return;
    setBusy(true);
    await onRun(prompt.trim(), flowKey).finally(() => setBusy(false));
  };
  return (
    <Dialog
      title={t("flows.run.title", { name: flowName })}
      onClose={onClose}
      footer={
        <>
          <Button variant="primary" disabled={!prompt.trim() || busy} onClick={() => void go()}>
            {t("flows.run.start")}
          </Button>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
        </>
      }
    >
      <label className="field">
        <span className="field__label">{t("flows.run.prompt")}</span>
        <textarea
          rows={4}
          value={prompt}
          placeholder={t("flows.run.placeholder")}
          onChange={(e) => setPrompt(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) void go();
          }}
        />
      </label>
      <p className="hint">{t("flows.run.hint")}</p>
    </Dialog>
  );
}

