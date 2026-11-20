// The workspace's side panel: a resizable column (drag its edge, arrows on the
// focused edge, double-click to maximize) with five views — Explorador, Buscar,
// Revisión, Terminal, Vista previa. The code views say which copy they show
// ("Tu carpeta" / "Tarea: …") and share one file viewer.

import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { useApi } from "../api/context";
import type { CodeCopy, CopyInfo, MatchRange } from "../api/codeTypes";
import type { Task } from "../api/types";
import { TerminalView } from "../components/Terminal/TerminalView";
import { clampSize, ResizeHandle } from "../components/ui/ResizeHandle";
import { Segmented } from "../components/ui/controls";
import { Select } from "../components/ui/Select";
import { TabBar, type TabItem } from "../components/ui/TabBar";
import { codeKeys, copyKey, sameCopy, useFileNavigator, type NavigatorTarget } from "../files/navigator";
import { useT } from "../i18n";
import { useReviewComments } from "../review/comments";
import { useTaskDiff } from "../tasks/hooks";
import { isOpen } from "../tasks/timeline";
import "../styles/code.css";
import "../styles/edit.css";
import { defaultCopy, taskHasCopy } from "./copies";
import { useUnsavedChanges } from "./editor/unsaved";
import { Explorer } from "./explorer/Explorer";
import { FilesIcon, LockIcon, PreviewIcon, ReviewIcon, TerminalIcon } from "./icons";
import { panelShortcut, shortcutAria, shortcutLabel, type PanelShortcut } from "./keys";

import { SearchView } from "./search/SearchView";
import { FileViewer, type OpenFile } from "./viewer/FileViewer";

export type PanelTab = "explorer" | "search" | "review" | "terminal" | "preview";

const WIDTH_KEY = "hedy.sidePanel.width";
const LIST_KEY = "hedy.sidePanel.listWidth";
/** The conversation never gets narrower than this, however wide the panel. */
export const MIN_CHAT = 460;
export const MIN_PANEL = 320;
const MIN_LIST = 180;
const MAX_LIST = 520;

function readNumber(key: string): number | null {
  try {
    const value = Number(localStorage.getItem(key));
    return Number.isFinite(value) && value > 0 ? value : null;
  } catch {
    return null;
  }
}

function writeNumber(key: string, value: number) {
  try {
    localStorage.setItem(key, String(Math.round(value)));
  } catch {
    // Remembering a width is only a convenience.
  }
}

/** The panel's default width for a workspace this wide. */
export function defaultPanelWidth(container: number): number {
  return clampSize(container * 0.4, MIN_PANEL, Math.min(720, container - MIN_CHAT));
}

/** The panel's width: remembered, kept within the workspace, maximizable. */
function usePanelWidth(container: number) {
  const [saved, setSaved] = useState<number | null>(() => readNumber(WIDTH_KEY));
  const [restore, setRestore] = useState<number | null>(null);
  const max = Math.max(MIN_PANEL, container - MIN_CHAT);
  const width = container > 0 ? clampSize(saved ?? defaultPanelWidth(container), MIN_PANEL, max) : (saved ?? null);
  const set = (value: number) => {
    setSaved(value);
    setRestore(null);
    writeNumber(WIDTH_KEY, value);
  };
  const maximized = width !== null && width >= max - 1;
  const toggleMax = () => {
    if (width === null) return;
    if (maximized) {
      const back = restore ?? defaultPanelWidth(container);
      setSaved(back);
      writeNumber(WIDTH_KEY, back);
      setRestore(null);
    } else {
      setRestore(width);
      setSaved(max);
      writeNumber(WIDTH_KEY, max);
    }
  };
  return { width, max, set, maximized, toggleMax };
}

/** The current text selection, as a one-line search ("" when none or several lines). */
function selectedText(): string {
  const text = window.getSelection?.()?.toString() ?? "";
  return text.includes("\n") ? "" : text.trim().slice(0, 200);
}

export interface SidePanelProps {
  project: string;
  task: Task | null;
  hidden?: boolean;
  /** Something asked to show the panel (a file opened, a shortcut). */
  onRequestOpen?: () => void;
  /** The Revisión tab's content. */
  review: ReactNode;
  /** The Vista previa tab's content. */
  preview: ReactNode;
}

export function SidePanel({ project, task, hidden = false, onRequestOpen, review, preview }: SidePanelProps) {
  const t = useT();
  const api = useApi();
  const idPrefix = useId();
  const nav = useFileNavigator();
  const reviewBridge = useReviewComments();
  const aside = useRef<HTMLElement>(null);
  const [container, setContainer] = useState(0);
  const panel = usePanelWidth(container);

  // The workspace's width bounds the panel's (the conversation keeps its minimum).
  useLayoutEffect(() => {
    const parent = aside.current?.parentElement;
    if (!parent) return;
    setContainer(parent.clientWidth);
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => setContainer(parent.clientWidth));
    observer.observe(parent);
    return () => observer.disconnect();
  }, []);

  // --- which copy, which file, which tab: per task (a new task starts fresh).
  const taskId = task?.id ?? null;
  const fresh = useCallback((): Browsing => ({ task: taskId, copy: null, file: null, viewing: false, tab: null }), [taskId]);
  const [state, setState] = useState<Browsing>(fresh);
  const current = state.task === taskId ? state : fresh();
  const copy = current.copy ?? defaultCopy(project, task);
  const file = current.file;
  const update = useCallback(
    (patch: Partial<Browsing> | ((old: Browsing) => Partial<Browsing>)) =>
      setState((old) => {
        const base = old.task === taskId ? old : fresh();
        return { ...base, ...(typeof patch === "function" ? patch(base) : patch) };
      }),
    [taskId, fresh],
  );

  const info = useQuery({ queryKey: codeKeys.info(copy), queryFn: () => api.codeCopy(copy), retry: false });
  // The open task's agent state is live (task events); the copy's info only when fetched.
  const working = !!task && (task.status === "running" || task.status === "waitingApproval");
  const ownTask = copy.kind === "task" && copy.task === taskId;
  const copyInfo = useMemo(
    () => (info.data && ownTask ? { ...info.data, agentWorking: working } : info.data),
    [info.data, ownTask, working],
  );
  // The agent stopped: its files may have changed (a clean viewer reloads; an edit is checked on save).
  const client = useQueryClient();
  const wasWorking = useRef(working);
  useEffect(() => {
    if (wasWorking.current && !working && taskId) {
      void client.invalidateQueries({ queryKey: codeKeys.copy({ kind: "task", task: taskId }) });
    }
    wasWorking.current = working;
  }, [working, taskId, client]);

  // Unsaved edits in the viewer: leaving the file asks first (Guardar / Descartar / Cancelar).
  const unsaved = useUnsavedChanges();
  const [dirtyFile, setDirtyFile] = useState<string | null>(null);
  const dirty = !!file && dirtyFile === file.path;
  const guard = (go: () => void) => {
    if (!dirty) return go();
    void unsaved.confirmLeave().then((ok) => ok && go());
  };

  // --- tabs
  const editable = !!task?.workspace && isOpen(task);
  const diff = useTaskDiff(task?.id ?? null, editable);
  const hasChanges = editable && (diff.data?.length ?? 0) > 0;
  // A task opens on its review (which also says why there is nothing to review yet); no task, on the files.
  const tab: PanelTab = current.tab ?? (task ? "review" : "explorer");
  const isCode = tab === "explorer" || tab === "search";
  // Views stay mounted once opened: the terminal keeps running, the search keeps its query.
  const [opened, setOpened] = useState<{ code: boolean; terminal: boolean }>({ code: false, terminal: false });
  if ((isCode && !opened.code) || (tab === "terminal" && !opened.terminal)) {
    setOpened({ code: opened.code || isCode, terminal: opened.terminal || tab === "terminal" });
  }
  const dir = task?.workspace?.worktree ?? project;

  const [focus, setFocus] = useState<{ view: PanelShortcut | null; seq: number; prefill?: string }>({ view: null, seq: 0 });
  const [reveal, setReveal] = useState<{ path: string; folder: boolean; seq: number } | null>(null);
  const revealIn = (path: string, folder: boolean) => setReveal((r) => ({ path, folder, seq: (r?.seq ?? 0) + 1 }));
  // Done outside this component once a request is applied: show the panel, select in the review.
  const [request, setRequest] = useState<{ seq: number; select?: string }>({ seq: 0 });
  const requestOpen = (select?: string) => setRequest((r) => ({ seq: r.seq + 1, select }));

  const show = (view: PanelTab, options?: { focus?: boolean; prefill?: string; select?: string }) => {
    update({ tab: view, viewing: view === "explorer" || view === "search" ? false : current.viewing });
    requestOpen(options?.select);
    if (options?.focus && (view === "explorer" || view === "search" || view === "review")) {
      setFocus((f) => ({ view, seq: f.seq + 1, prefill: options.prefill }));
    }
  };

  const openFile = (next: Omit<OpenFile, "seq">, options?: { copy?: CodeCopy; tab?: PanelTab }) => {
    update((base) => ({
      copy: options?.copy ?? base.copy,
      file: { ...next, seq: (base.file?.seq ?? 0) + 1 },
      viewing: true,
      tab: options?.tab ?? (base.tab === "explorer" || base.tab === "search" ? base.tab : "explorer"),
    }));
    requestOpen();
  };

  const showDiff = (path: string, inCopy: CodeCopy) => {
    // The review is of the open task; elsewhere, the viewer compares with the base.
    if (inCopy.kind === "task" && inCopy.task === taskId && taskHasCopy(task) && isOpen(task)) {
      show("review", { select: path });
    } else {
      navigate(path, { compare: "base" }, inCopy);
    }
  };

  // Every file opens through the navigator (so the palette knows the recent ones),
  // with what only this panel uses (search hits, the history drawer) kept aside.
  const [extra, setExtra] = useState<{ path: string; more: Partial<OpenFile> } | null>(null);
  const navigate = (path: string, more: Partial<OpenFile> = {}, inCopy: CodeCopy = copy) => {
    setExtra({ path, more });
    nav.open({ copy: inCopy, path, line: more.line, rev: more.rev });
  };

  const applyTarget = (target: NavigatorTarget & { view: string }, more: Partial<OpenFile>) => {
    openFile(
      {
        ...more,
        path: target.path,
        line: target.line,
        rev: target.rev,
        compare: target.view === "diff" ? (target.rev ?? "base") : more.compare,
        edit: target.edit || more.edit,
      },
      { copy: target.copy, tab: tab === "search" ? "search" : "explorer" },
    );
    if (tab !== "search") revealIn(target.path, false);
  };

  // A request for another file while this one has unsaved changes: held until answered.
  const [held, setHeld] = useState<{ target: NavigatorTarget & { view: string }; more: Partial<OpenFile> } | null>(null);
  useEffect(() => {
    if (!held) return;
    let live = true;
    void unsaved.confirmLeave().then((ok) => {
      if (!live) return;
      setHeld(null);
      if (ok) applyTarget(held.target, held.more);
    });
    return () => {
      live = false;
    };
    // Only a new held request asks.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [held]);

  // Requests from here and from elsewhere (a link in the timeline, the palette, a diff line).
  const [navSeen, setNavSeen] = useState(nav.seq);
  if (nav.seq !== navSeen) {
    setNavSeen(nav.seq);
    const target = nav.current;
    const more = extra && target && extra.path === target.path ? extra.more : {};
    if (extra) setExtra(null);
    if (target?.view === "diff" && target.copy.kind === "task" && target.copy.task === taskId) {
      // The review store selects the file itself (it listens to the navigator too).
      show("review");
    } else if (target) {
      const sameFile = !!file && sameCopy(target.copy, copy) && target.path === file.path;
      if (dirty && !sameFile) setHeld({ target, more });
      else applyTarget(target, more);
    }
  }

  // Applied: show the panel (and select in the review) outside this component.
  useEffect(() => {
    if (request.seq === 0) return;
    onRequestOpen?.();
    if (request.select) reviewBridge.selectFile(request.select);
    // Only a new request matters, not new callbacks.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request]);

  // Ctrl/⌘ Shift E / F / G, from anywhere in the workspace (not a terminal).
  const showRef = useRef(show);
  useLayoutEffect(() => {
    showRef.current = show;
  });
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof Element && e.target.closest(".xterm")) return;
      if (document.querySelector('[aria-modal="true"]')) return;
      const view = panelShortcut(e);
      if (!view) return;
      e.preventDefault();
      showRef.current(view, { focus: true, prefill: view === "search" ? selectedText() || undefined : undefined });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // The review tab's content gets the focus after Ctrl/⌘ Shift G.
  const reviewPanel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (focus.view !== "review" || focus.seq === 0) return;
    const el = reviewPanel.current;
    (el?.querySelector<HTMLElement>("button, [href], input, [tabindex]:not([tabindex='-1'])") ?? el)?.focus();
  }, [focus]);

  // --- the copy selector's options: the folder, the open task, any other task shown.
  const taskTitle = (id: string) => (task?.id === id ? task.title : info.data?.task === id ? info.data.title : null) ?? id;
  const copies: CodeCopy[] = [{ kind: "folder", project }];
  if (taskHasCopy(task)) copies.push({ kind: "task", task: task.id });
  if (!copies.some((c) => sameCopy(c, copy))) copies.push(copy);
  const copyLabel = (c: CodeCopy) => (c.kind === "folder" ? t("code.copy.folder") : t("code.copy.task", { title: taskTitle(c.task) }));

  const tabs: TabItem<PanelTab>[] = [
    {
      id: "review",
      label: hasChanges ? t("code.tab.changesN", { n: diff.data?.length ?? 0 }) : t("code.tab.review"),
      icon: <ReviewIcon />,
      shortcut: shortcutLabel("review"),
      keyshortcuts: shortcutAria("review"),
    },
    // The files and their search share a tab: the search is a view of the files.
    {
      id: "explorer",
      label: t("code.tab.explorer"),
      icon: <FilesIcon />,
      shortcut: shortcutLabel("explorer"),
      keyshortcuts: shortcutAria("explorer"),
      controls: tab === "search" ? `${idPrefix}-panel-search` : undefined,
    },
    { id: "terminal", label: t("common.terminal"), icon: <TerminalIcon /> },
    { id: "preview", label: t("workspace.tab.preview"), icon: <PreviewIcon /> },
  ];

  const viewer = file ? (
    <FileViewer
      key={`${copyKey(copy)}:${file.path}`}
      copy={copy}
      info={copyInfo}
      file={file}
      onChange={(next) => update({ file: next })}
      onRevealFolder={(folder) => {
        update({ tab: "explorer", viewing: false });
        revealIn(folder, true);
      }}
      onShowDiff={(path) => showDiff(path, copy)}
      onOpenTask={(touch, path) => navigate(path, {}, { kind: "task", task: touch.task })}
      onBack={() => update({ viewing: false })}
      onClose={() => guard(() => update({ file: null, viewing: false }))}
      onDirtyChange={(on) => setDirtyFile(on ? file.path : null)}
    />
  ) : null;

  const width = panel.width;
  return (
    <aside
      ref={aside}
      className={`side-panel${panel.maximized ? " side-panel--max" : ""}`}
      aria-label={t("workspace.panel")}
      hidden={hidden}
      style={width !== null ? { width } : undefined}
    >
      <ResizeHandle
        className="side-panel__edge"
        label={t("code.panel.resize")}
        title={t("code.panel.resizeHint")}
        value={width ?? MIN_PANEL}
        min={MIN_PANEL}
        max={panel.max}
        onChange={panel.set}
        onToggleMax={panel.toggleMax}
      />
      <TabBar
        label={t("code.panel.tabs")}
        tabs={tabs}
        value={tab === "search" ? "explorer" : tab}
        onChange={(next) => update({ tab: next === "explorer" && tab === "search" ? "search" : next, viewing: false })}
        idPrefix={idPrefix}
      />
      <div className="side-panel__views">
        {opened.code && (
          <div className="side-view side-view--code" hidden={!isCode}>
            <div className="files-mode">
              <Segmented<"explorer" | "search">
                label={t("code.tab.explorer")}
                value={tab === "search" ? "search" : "explorer"}
                onChange={(view) => show(view, { focus: view === "search" })}
                options={[
                  { value: "explorer", label: t("code.files.tree") },
                  { value: "search", label: `${t("code.tab.search")} · ${shortcutLabel("search")}` },
                ]}
              />
            </div>
            <CopyBar
              copies={copies}
              copy={copy}
              label={copyLabel}
              info={copyInfo}
              error={info.isError}
              onChange={(c) => guard(() => update({ copy: c }))}
            />
            <Browse viewing={!!file && current.viewing} viewer={viewer}>
              <div
                role="tabpanel"
                id={`${idPrefix}-panel-explorer`}
                aria-labelledby={`${idPrefix}-tab-explorer`}
                className="browse__pane"
                hidden={tab !== "explorer"}
              >
                <Explorer
                  copy={copy}
                  info={copyInfo}
                  openPath={file?.path ?? null}
                  dirtyPath={dirty ? (file?.path ?? null) : null}
                  onCreated={(path) => navigate(path, { edit: true })}
                  onMoved={(from, to) => {
                    const open = file?.path;
                    if (!open || !(open === from || open.startsWith(`${from}/`))) return;
                    update({ file: to === null ? null : { ...file, path: `${to}${open.slice(from.length)}`, seq: file.seq + 1 } });
                  }}
                  onOpen={(path) => navigate(path)}
                  onShowDiff={(path) => showDiff(path, copy)}
                  onHistory={(path) => navigate(path, { history: true })}
                  revealRequest={reveal}
                  focusSeq={focus.view === "explorer" ? focus.seq : 0}
                />
              </div>
              <div
                role="tabpanel"
                id={`${idPrefix}-panel-search`}
                aria-labelledby={`${idPrefix}-tab-explorer`}
                className="browse__pane"
                hidden={tab !== "search"}
              >
                <SearchView
                  copy={copy}
                  info={copyInfo}
                  openKey={file?.select ?? null}
                  onOpen={(path: string, line: number, ranges: MatchRange[], key: string) =>
                    navigate(path, { line, ranges, select: key })
                  }
                  focusSeq={focus.view === "search" ? focus.seq : 0}
                  prefill={focus.view === "search" ? focus.prefill : undefined}
                />
              </div>
            </Browse>
          </div>
        )}
        {tab === "review" && (
          <div
            ref={reviewPanel}
            role="tabpanel"
            id={`${idPrefix}-panel-review`}
            aria-labelledby={`${idPrefix}-tab-review`}
            className="side-view side-view--scroll"
            tabIndex={-1}
          >
            {review}
          </div>
        )}
        {opened.terminal && (
          // One terminal per folder; it opens in the task's worktree.
          <div
            role="tabpanel"
            id={`${idPrefix}-panel-terminal`}
            aria-labelledby={`${idPrefix}-tab-terminal`}
            className="side-view side-terminal"
            hidden={tab !== "terminal"}
          >
            <TerminalView key={dir} request={{ kind: "shell", path: dir }} active={tab === "terminal"} onExit={() => {}} />
          </div>
        )}
        {tab === "preview" && (
          <div
            role="tabpanel"
            id={`${idPrefix}-panel-preview`}
            aria-labelledby={`${idPrefix}-tab-preview`}
            className="side-view side-view--scroll"
          >
            {preview}
          </div>
        )}
      </div>
    </aside>
  );
}

/** What the panel shows for the open task. */
interface Browsing {
  task: string | null;
  /** `null`: the default copy (the task's own, else the folder). */
  copy: CodeCopy | null;
  file: OpenFile | null;
  /** Narrow panels show the list or the viewer; this says which. */
  viewing: boolean;
  /** `null`: the default tab (the review while there are changes). */
  tab: PanelTab | null;
}

/** "Tu carpeta" / "Tarea: …", with a snapshot's and a working agent's note. */
function CopyBar({
  copies,
  copy,
  label,
  info,
  error,
  onChange,
}: {
  copies: CodeCopy[];
  copy: CodeCopy;
  label: (copy: CodeCopy) => string;
  info: CopyInfo | undefined;
  error: boolean;
  onChange: (copy: CodeCopy) => void;
}) {
  const t = useT();
  const byKey = new Map(copies.map((c) => [copyKey(c), c]));
  return (
    <div className="copy-bar">
      {copies.length > 1 ? (
        <Select
          variant="pill"
          className="copy-bar__select"
          label={t("code.copy.label")}
          title={t("code.copy.label")}
          value={copyKey(copy)}
          options={copies.map((c) => ({
            value: copyKey(c),
            label: label(c),
            detail: c.kind === "folder" ? t("code.copy.folderDetail") : t("code.copy.taskDetail"),
          }))}
          onChange={(key) => {
            const next = byKey.get(key);
            if (next) onChange(next);
          }}
        />
      ) : (
        <span className="copy-bar__name">{label(copy)}</span>
      )}
      {info && !info.live && (
        <span className="copy-bar__badge" title={t("code.copy.snapshotHint")}>
          <LockIcon size={12} />
          {t("code.copy.snapshot")}
        </span>
      )}
      {info?.agentWorking && (
        <span className="copy-bar__badge copy-bar__badge--working" title={t("code.copy.workingHint")}>
          <span className="copy-bar__pulse" aria-hidden />
          {t("code.copy.working")}
        </span>
      )}
      {error && <span className="copy-bar__badge copy-bar__badge--error">{t("code.copy.unavailable")}</span>}
    </div>
  );
}

/**
 * A list beside the viewer when the panel is wide; one or the other when it
 * is narrow (a container query decides, so no measuring here).
 */
function Browse({ viewing, viewer, children }: { viewing: boolean; viewer: ReactNode; children: ReactNode }) {
  const t = useT();
  const [list, setList] = useState(() => readNumber(LIST_KEY) ?? 260);
  return (
    <div className={`browse${viewer ? " browse--with-viewer" : ""}${viewing ? " browse--viewing" : ""}`}>
      <div className="browse__list" style={viewer ? { width: list } : undefined}>
        {children}
      </div>
      {viewer && (
        <>
          <ResizeHandle
            className="browse__edge"
            label={t("code.panel.resizeList")}
            value={list}
            min={MIN_LIST}
            max={MAX_LIST}
            direction={1}
            onChange={(value) => {
              setList(value);
              writeNumber(LIST_KEY, value);
            }}
          />
          <div className="browse__viewer">{viewer}</div>
        </>
      )}
    </div>
  );
}
