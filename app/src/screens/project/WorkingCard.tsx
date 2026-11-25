// «Cómo trabaja en este proyecto» (docs/decisions.md §60): a preset by name, and
// the stops it is made of. One AI in one session; Hedy runs the checks.

import { useId, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";

import { OVERVIEW_KEY, useAction, useApi } from "../../api/context";
import type { Project, WorkingStyle } from "../../api/types";
import { Switch } from "../../components/ui/controls";
import { useT, type MessageKey } from "../../i18n";
import { applyPreset, PRESETS, presetOf, type Preset } from "./working";

const PRESET_TEXT: Record<Preset, { name: MessageKey; detail: MessageKey }> = {
  now: { name: "workspace.composer.now", detail: "projectSettings.working.nowDetail" },
  afterOk: { name: "projectSettings.working.afterOk", detail: "projectSettings.working.afterOkDetail" },
  afterOkReview: { name: "projectSettings.working.afterOkReview", detail: "projectSettings.working.afterOkReviewDetail" },
};

const HOW: MessageKey[] = [
  "projectSettings.working.how.session",
  "projectSettings.working.how.checks",
  "projectSettings.working.how.never",
  "projectSettings.working.how.flows",
];

export function WorkingCard({ project, more }: { project: Project; more: ReactNode }) {
  const t = useT();
  const api = useApi();
  const titleId = useId();
  const howId = useId();
  const moreId = useId();
  const [howOpen, setHowOpen] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  const style = project.working;
  const preset = presetOf(style);
  const save = useAction((a, next: WorkingStyle) => a.setProjectWorking(project.path, next), { invalidate: [OVERVIEW_KEY] });
  const detected = useQuery({
    queryKey: ["detectedTests", project.path],
    queryFn: () => api.detectedTestsCommand(project.path),
  });
  const change = (patch: Partial<WorkingStyle>) => save.mutate({ ...style, ...patch });

  return (
    <section className="card working" aria-labelledby={titleId}>
      <div className="row row--between">
        <h2 id={titleId}>{t("projectSettings.working.title")}</h2>
        {!preset && <span className="working__custom">{t("projectSettings.working.custom")}</span>}
      </div>
      <p className="small muted">{t("projectSettings.working.text")}</p>
      <div className="working__presets" role="radiogroup" aria-labelledby={titleId}>
        {PRESETS.map((p) => (
          <button
            key={p}
            type="button"
            role="radio"
            aria-checked={preset === p}
            className="working__preset"
            disabled={save.isPending}
            onClick={() => save.mutate(applyPreset(p, style))}
          >
            <span className="working__preset-name">{t(PRESET_TEXT[p].name)}</span>
            <span className="working__preset-detail">{t(PRESET_TEXT[p].detail)}</span>
          </button>
        ))}
      </div>

      <ul className="working__stops">
        <Stop label={t("projectSettings.working.plan")} checked={style.plan} disabled={save.isPending} onChange={(plan) => change({ plan })} />
        <Stop label={t("projectSettings.working.tests")} checked={style.tests} disabled={save.isPending} onChange={(tests) => change({ tests })}>
          <CommandField
            label={t("projectSettings.working.testsCommand")}
            value={style.testsCommand ?? ""}
            placeholder={
              detected.data
                ? t("projectSettings.working.detected", { command: detected.data })
                : t("projectSettings.working.notDetected")
            }
            onSave={(testsCommand) => change({ testsCommand: testsCommand || null })}
          />
        </Stop>
        <Stop label={t("projectSettings.working.e2e")} checked={style.e2e} disabled={save.isPending} onChange={(e2e) => change({ e2e })}>
          <CommandField
            label={t("projectSettings.working.e2eCommand")}
            value={style.e2eCommand ?? ""}
            placeholder={t("projectSettings.working.e2ePlaceholder")}
            hint={t("projectSettings.working.e2eHint")}
            onSave={(e2eCommand) => change({ e2eCommand: e2eCommand || null })}
          />
        </Stop>
        <Stop label={t("projectSettings.working.review")} checked={style.review} disabled={save.isPending} onChange={(review) => change({ review })}>
          <p className="small muted">{t("projectSettings.working.reviewHint")}</p>
        </Stop>
      </ul>

      <button type="button" className="disclosure disclosure--quiet" aria-expanded={howOpen} aria-controls={howId} onClick={() => setHowOpen((o) => !o)}>
        <span className="disclosure__mark" aria-hidden>
          ›
        </span>
        {t("settings.saving.how")}
      </button>
      <ul id={howId} className="settings__how small muted" hidden={!howOpen}>
        {HOW.map((key) => (
          <li key={key}>{t(key)}</li>
        ))}
      </ul>
      <button type="button" className="disclosure disclosure--quiet" aria-expanded={moreOpen} aria-controls={moreId} onClick={() => setMoreOpen((o) => !o)}>
        <span className="disclosure__mark" aria-hidden>
          ›
        </span>
        {t("projectSettings.working.more")}
      </button>
      <div id={moreId} hidden={!moreOpen}>
        {moreOpen && more}
      </div>
    </section>
  );
}

function Stop({
  label,
  checked,
  disabled,
  onChange,
  children,
}: {
  label: string;
  checked: boolean;
  disabled: boolean;
  onChange: (on: boolean) => void;
  children?: ReactNode;
}) {
  return (
    <li className="working__stop">
      <div className="row row--between">
        <span>{label}</span>
        <Switch label={label} checked={checked} disabled={disabled} onChange={onChange} />
      </div>
      {checked && children}
    </li>
  );
}

/** A command saved when the field is left (Enter too). */
function CommandField({
  label,
  value,
  placeholder,
  hint,
  onSave,
}: {
  label: string;
  value: string;
  placeholder: string;
  hint?: string;
  onSave: (command: string) => void;
}) {
  const id = useId();
  const hintId = useId();
  const [text, setText] = useState(value);
  const commit = () => {
    if (text.trim() !== value) onSave(text.trim());
  };
  return (
    <div className="field working__command">
      <label className="sr-only" htmlFor={id}>
        {label}
      </label>
      <input
        id={id}
        className="mono"
        value={text}
        placeholder={placeholder}
        aria-describedby={hint ? hintId : undefined}
        spellCheck={false}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit();
          }
        }}
      />
      {hint && (
        <p id={hintId} className="small muted">
          {hint}
        </p>
      )}
    </div>
  );
}
