// The panel beside the canvas: the selected step's settings and where it goes
// next, the selected connection, or (nothing selected) the flow itself.

import { useState, type ReactNode } from "react";

import type { Account, Flow, FlowCheck, FlowIssue, FlowNode, FlowWhen, RoleDef, Seat } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { Field } from "../../components/ui/controls";
import { Select, type SelectOption } from "../../components/ui/Select";
import { useT, type MessageKey } from "../../i18n";
import { connect, disconnect, edgeFrom, forward, MAX_NAME, MAX_ROUNDS, nodeById, setRounds, setStep } from "./flowModel";
import type { Selection } from "./FlowCanvas";
import { StepIcon, stepTint } from "./StepIcon";
import { useStepText } from "./stepText";

/** `merge`: edits with the same key in a row are one undo step (typing). */
export type Change = (flow: Flow, merge?: string) => void;

export function FlowInspector({
  flow,
  selection,
  roles,
  accounts,
  issues,
  readOnly,
  onChange,
  onSelect,
  onRemove,
}: {
  flow: Flow;
  selection: Selection;
  roles: RoleDef[];
  accounts: Account[];
  issues: FlowIssue[];
  readOnly: boolean;
  onChange: Change;
  onSelect: (selection: Selection) => void;
  onRemove: (node: string) => void;
}) {
  const node = selection && "node" in selection ? nodeById(flow, selection.node) : undefined;
  const edge = selection && "edge" in selection ? edgeFrom(flow, selection.edge.from, selection.edge.when) : undefined;
  return (
    <aside className="flow-inspector" aria-label={useT()("flows.inspector")} onPointerDown={(e) => e.stopPropagation()}>
      {node ? (
        <StepPanel
          key={node.id}
          flow={flow}
          node={node}
          roles={roles}
          accounts={accounts}
          issues={issues.filter((i) => i.node === node.id)}
          readOnly={readOnly}
          onChange={onChange}
          onClose={() => onSelect(null)}
          onRemove={() => onRemove(node.id)}
        />
      ) : edge ? (
        <EdgePanel flow={flow} from={edge.from} when={edge.when} roles={roles} readOnly={readOnly} onChange={onChange} onClose={() => onSelect(null)} />
      ) : (
        <FlowPanel flow={flow} issues={issues} readOnly={readOnly} onChange={onChange} onSelect={onSelect} />
      )}
    </aside>
  );
}

function PanelHead({ icon, title, subtitle, onClose }: { icon?: ReactNode; title: string; subtitle?: string; onClose?: () => void }) {
  const t = useT();
  return (
    <header className="flow-inspector__head">
      {icon}
      <div className="flow-inspector__titles">
        <h2>{title}</h2>
        {subtitle && <p>{subtitle}</p>}
      </div>
      {onClose && (
        <button type="button" className="flow-inspector__close" aria-label={t("flows.inspector.close")} onClick={onClose}>
          <svg aria-hidden="true" width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
            <path d="M3 3l6 6M9 3l-6 6" />
          </svg>
        </button>
      )}
    </header>
  );
}

function Issues({ issues }: { issues: FlowIssue[] }) {
  if (!issues.length) return null;
  return (
    <ul className="flow-issues" role="list">
      {issues.map((i, n) => (
        <li key={`${i.node ?? ""}-${n}`}>{i.message}</li>
      ))}
    </ul>
  );
}

/** Lines of a textarea as a list (empty lines dropped). */
const lines = (text: string) => text.split("\n").map((l) => l.trim()).filter(Boolean);

/** A list edited as lines: the text stays as typed while it means the same list. */
function LinesField({
  label,
  placeholder,
  mono,
  value,
  readOnly,
  onChange,
}: {
  label: string;
  placeholder: string;
  mono?: boolean;
  value: string[];
  readOnly: boolean;
  onChange: (value: string[]) => void;
}) {
  const t = useT();
  const [text, setText] = useState(value.join("\n"));
  // An undo, or another flow: the list changed from outside.
  if (lines(text).join("\n") !== value.join("\n")) setText(value.join("\n"));
  return (
    <Field label={label} hint={t("flows.frame.oneLine")}>
      {(id) => (
        <textarea
          id={id}
          className={mono ? "input--mono" : undefined}
          rows={mono ? 2 : 3}
          placeholder={placeholder}
          value={text}
          readOnly={readOnly}
          onChange={(e) => {
            setText(e.target.value);
            onChange(lines(e.target.value));
          }}
        />
      )}
    </Field>
  );
}

function FlowPanel({
  flow,
  issues,
  readOnly,
  onChange,
  onSelect,
}: {
  flow: Flow;
  issues: FlowIssue[];
  readOnly: boolean;
  onChange: Change;
  onSelect: (selection: Selection) => void;
}) {
  const t = useT();
  const frame = flow.frame;
  const setFrame = (patch: Partial<Flow["frame"]>, merge: string) => onChange({ ...flow, frame: { ...frame, ...patch } }, merge);
  return (
    <>
      <PanelHead title={t("flows.inspector.flow")} subtitle={t("flows.inspector.flowHint")} />
      {issues.length > 0 && (
        <section className="flow-inspector__section">
          <h3>{t("flows.issues.title", { count: String(issues.length) })}</h3>
          <ul className="flow-issues" role="list">
            {issues.map((i, n) => (
              <li key={`${i.node ?? ""}-${n}`}>
                {i.node ? (
                  <button type="button" className="link-button" onClick={() => i.node && onSelect({ node: i.node })}>
                    {i.message}
                  </button>
                ) : (
                  i.message
                )}
              </li>
            ))}
          </ul>
        </section>
      )}
      <div className="flow-inspector__section">
        <Field label={t("flows.field.name")}>
          {(id) => (
            <input
              id={id}
              value={flow.name}
              maxLength={MAX_NAME}
              readOnly={readOnly}
              onChange={(e) => onChange({ ...flow, name: e.target.value.replace(/[\r\n]/g, "") }, "name")}
            />
          )}
        </Field>
      </div>
      <section className="flow-inspector__section">
        <h3>{t("flows.frame.title")}</h3>
        <p className="hint">{t("flows.frame.hint")}</p>
        <Field label={t("flows.frame.goal")}>
          {(id) => (
            <textarea
              id={id}
              rows={2}
              placeholder={t("flows.frame.goalPlaceholder")}
              value={frame.goal ?? ""}
              readOnly={readOnly}
              onChange={(e) => setFrame({ goal: e.target.value }, "goal")}
            />
          )}
        </Field>
        <LinesField
          label={t("flows.frame.rules")}
          placeholder={t("flows.frame.rulesPlaceholder")}
          value={frame.rules ?? []}
          readOnly={readOnly}
          onChange={(rules) => setFrame({ rules }, "rules")}
        />
        <LinesField
          label={t("flows.frame.files")}
          placeholder="src/pagos/"
          mono
          value={frame.files ?? []}
          readOnly={readOnly}
          onChange={(files) => setFrame({ files }, "files")}
        />
      </section>
    </>
  );
}

function StepPanel({
  flow,
  node,
  roles,
  accounts,
  issues,
  readOnly,
  onChange,
  onClose,
  onRemove,
}: {
  flow: Flow;
  node: FlowNode;
  roles: RoleDef[];
  accounts: Account[];
  issues: FlowIssue[];
  readOnly: boolean;
  onChange: Change;
  onClose: () => void;
  onRemove: () => void;
}) {
  const t = useT();
  const text = useStepText(roles)(node);
  const role = node.kind === "role" ? roles.find((r) => r.id === node.role) : undefined;
  const kind: MessageKey =
    node.kind === "role"
      ? "flows.kind.role"
      : node.kind === "condition"
        ? "flows.kind.condition"
        : node.kind === "parallel"
          ? "flows.kind.parallel"
          : node.kind === "approval"
            ? "flows.kind.approval"
            : node.kind === "start"
              ? "flows.kind.start"
              : "flows.kind.end";
  return (
    <>
      <PanelHead
        icon={
          <span className="flow-inspector__icon" style={{ ["--tint" as string]: stepTint(node, roles) }}>
            <StepIcon node={node} roles={roles} size={20} />
          </span>
        }
        title={text.name}
        subtitle={t(kind)}
        onClose={onClose}
      />
      <Issues issues={issues} />
      <div className="flow-inspector__section">
        {node.kind === "role" && (
          <SeatFields
            seat={node}
            roles={roles}
            accounts={accounts}
            readOnly={readOnly}
            onChange={(seat, merge) => onChange(setStep(flow, node.id, { kind: "role", ...seat }), merge && `${node.id}-${merge}`)}
          />
        )}
        {node.kind === "condition" && <CheckFields flow={flow} node={node} readOnly={readOnly} onChange={onChange} />}
        {node.kind === "parallel" && <ParallelFields flow={flow} node={node} roles={roles} readOnly={readOnly} onChange={onChange} />}
        {(node.kind === "approval" || node.kind === "start" || node.kind === "end") && (
          <p className="flow-inspector__explain">{t(`flows.explain.${node.kind}` as MessageKey)}</p>
        )}
      </div>
      {role && (
        <section className="flow-inspector__section">
          <h3>{t("flows.role.does")}</h3>
          <ul className="flow-facts" role="list">
            <li>{t(role.can.write ? "flows.role.writes" : "flows.role.reads")}</li>
            {role.can.run && <li>{t("flows.role.runs")}</li>}
            {role.delivers === "verdict" && <li>{t("flows.role.verdict")}</li>}
          </ul>
          <details className="flow-inspector__details">
            <summary>{t("flows.role.instructions")}</summary>
            <p>{role.instructions}</p>
          </details>
        </section>
      )}
      {node.kind !== "end" && <Ways flow={flow} node={node} roles={roles} readOnly={readOnly} onChange={onChange} />}
      {!readOnly && node.kind !== "start" && node.kind !== "end" && (
        <div className="flow-inspector__footer">
          <Button variant="ghost" small className="btn--danger-text" onClick={onRemove}>
            {t("flows.step.remove")}
          </Button>
        </div>
      )}
    </>
  );
}

function SeatFields({
  seat,
  roles,
  accounts,
  readOnly,
  onlyReaders,
  onChange,
}: {
  seat: Seat;
  roles: RoleDef[];
  accounts: Account[];
  readOnly: boolean;
  /** A group at once only takes roles that read. */
  onlyReaders?: boolean;
  onChange: (seat: Seat, merge?: string) => void;
}) {
  const t = useT();
  const options: SelectOption[] = roles
    .filter((r) => !onlyReaders || !r.can.write)
    .map((r) => ({ value: r.id, label: r.name, detail: r.summary }));
  return (
    <>
      <Field label={t("flows.field.role")}>
        {(id) => <Select id={id} variant="field" value={seat.role} options={options} disabled={readOnly} onChange={(role) => onChange({ ...seat, role })} />}
      </Field>
      <div className="flow-inspector__pair">
        <Field label={t("flows.field.model")}>
          {(id) => (
            <input
              id={id}
              value={seat.model ?? ""}
              placeholder={t("flows.field.projectDefault")}
              readOnly={readOnly}
              spellCheck={false}
              onChange={(e) => {
                const model = e.target.value.trim();
                const rest = { ...seat };
                delete rest.model;
                onChange(model ? { ...rest, model } : rest, "model");
              }}
            />
          )}
        </Field>
        <Field label={t("flows.field.account")}>
          {(id) => (
            <Select
              id={id}
              variant="field"
              value={seat.account ?? ""}
              disabled={readOnly}
              options={[{ value: "", label: t("flows.field.projectDefault") }, ...accounts.map((a) => ({ value: a.id, label: a.name, detail: a.providerName }))]}
              onChange={(account) => {
                const rest = { ...seat };
                delete rest.account;
                onChange(account ? { ...rest, account } : rest);
              }}
            />
          )}
        </Field>
      </div>
    </>
  );
}

function CheckFields({ flow, node, readOnly, onChange }: { flow: Flow; node: FlowNode & { kind: "condition" }; readOnly: boolean; onChange: Change }) {
  const t = useT();
  const set = (check: FlowCheck, merge?: string) => onChange(setStep(flow, node.id, { kind: "condition", ...check }), merge);
  return (
    <>
      <Field label={t("flows.field.check")}>
        {(id) => (
          <Select
            id={id}
            variant="field"
            value={node.check}
            disabled={readOnly}
            options={[
              { value: "tests", label: t("flows.step.tests"), detail: t("flows.check.testsDetail") },
              { value: "rules", label: t("flows.step.rules"), detail: t("flows.check.rulesDetail") },
              { value: "verdict", label: t("flows.step.verdict"), detail: t("flows.check.verdictDetail") },
            ]}
            onChange={(check) => set(check === "tests" ? { check } : { check })}
          />
        )}
      </Field>
      {node.check === "tests" && (
        <Field label={t("flows.field.command")} hint={t("flows.field.commandHint")}>
          {(id) => (
            <input
              id={id}
              className="input--mono"
              value={node.command ?? ""}
              placeholder={t("flows.field.commandPlaceholder")}
              readOnly={readOnly}
              spellCheck={false}
              onChange={(e) => {
                const command = e.target.value.replace(/[\r\n]/g, "");
                set(command.trim() ? { check: "tests", command } : { check: "tests" }, `${node.id}-command`);
              }}
            />
          )}
        </Field>
      )}
    </>
  );
}

function ParallelFields({
  flow,
  node,
  roles,
  readOnly,
  onChange,
}: {
  flow: Flow;
  node: FlowNode & { kind: "parallel" };
  roles: RoleDef[];
  readOnly: boolean;
  onChange: Change;
}) {
  const t = useT();
  const set = (seats: Seat[]) => onChange(setStep(flow, node.id, { kind: "parallel", roles: seats }));
  const readers = roles.filter((r) => !r.can.write);
  return (
    <>
      <p className="hint">{t("flows.parallel.hint")}</p>
      <ul className="flow-seats" role="list">
        {node.roles.map((seat, i) => (
          <li key={i}>
            <Select
              variant="field"
              label={t("flows.parallel.role", { n: String(i + 1) })}
              value={seat.role}
              disabled={readOnly}
              options={readers.map((r) => ({ value: r.id, label: r.name, detail: r.summary }))}
              onChange={(role) => set(node.roles.map((s, j) => (j === i ? { ...s, role } : s)))}
            />
            {!readOnly && node.roles.length > 2 && (
              <button
                type="button"
                className="flow-seats__remove"
                aria-label={t("flows.parallel.remove", { name: roles.find((r) => r.id === seat.role)?.name ?? seat.role })}
                onClick={() => set(node.roles.filter((_, j) => j !== i))}
              >
                <svg aria-hidden="true" width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
                  <path d="M3 3l6 6M9 3l-6 6" />
                </svg>
              </button>
            )}
          </li>
        ))}
      </ul>
      {!readOnly && readers[0] && (
        <Button small variant="ghost" onClick={() => readers[0] && set([...node.roles, { role: readers[0].id }])}>
          {t("flows.parallel.add")}
        </Button>
      )}
    </>
  );
}

/** Where the step goes next and, for a check, where a failure goes back to. */
function Ways({ flow, node, roles, readOnly, onChange }: { flow: Flow; node: FlowNode; roles: RoleDef[]; readOnly: boolean; onChange: Change }) {
  const t = useT();
  const text = useStepText(roles);
  const NONE = "";
  const targets = (when: FlowWhen): SelectOption[] => [
    { value: NONE, label: t("flows.ways.nowhere") },
    ...flow.nodes
      .filter((n) => n.id !== node.id && n.kind !== "start" && (when === "fail" ? n.kind !== "end" : true))
      .map((n) => ({ value: n.id, label: text(n).name, detail: text(n).detail || undefined })),
  ];
  const way = (when: FlowWhen, label: MessageKey) => {
    const edge = edgeFrom(flow, node.id, when);
    return (
      <Field label={t(label)}>
        {(id) => (
          <Select
            id={id}
            variant="field"
            value={edge?.to ?? NONE}
            disabled={readOnly}
            options={targets(when)}
            onChange={(to) => onChange(to ? connect(flow, node.id, when, to) : disconnect(flow, node.id, when))}
          />
        )}
      </Field>
    );
  };
  const fail = edgeFrom(flow, node.id, "fail");
  return (
    <section className="flow-inspector__section">
      <h3>{t("flows.ways.title")}</h3>
      {way(forward(node), node.kind === "condition" ? "flows.ways.pass" : "flows.ways.next")}
      {node.kind === "condition" && way("fail", node.check === "verdict" ? "flows.ways.changes" : "flows.ways.fail")}
      {fail && <Rounds flow={flow} from={node.id} rounds={fail.maxRounds} readOnly={readOnly} onChange={onChange} />}
    </section>
  );
}

function Rounds({ flow, from, rounds, readOnly, onChange }: { flow: Flow; from: string; rounds?: number; readOnly: boolean; onChange: Change }) {
  const t = useT();
  return (
    <Field label={t("flows.ways.rounds")} hint={t("flows.ways.roundsHint")}>
      {(id) => (
        <input
          id={id}
          className="flow-inspector__number"
          type="number"
          min={1}
          max={MAX_ROUNDS}
          value={rounds ?? ""}
          readOnly={readOnly}
          onChange={(e) => onChange(setRounds(flow, from, "fail", Number(e.target.value)), `${from}-rounds`)}
        />
      )}
    </Field>
  );
}

function EdgePanel({
  flow,
  from,
  when,
  roles,
  readOnly,
  onChange,
  onClose,
}: {
  flow: Flow;
  from: string;
  when: FlowWhen;
  roles: RoleDef[];
  readOnly: boolean;
  onChange: Change;
  onClose: () => void;
}) {
  const t = useT();
  const text = useStepText(roles);
  const edge = edgeFrom(flow, from, when);
  if (!edge) return null;
  const a = nodeById(flow, from);
  const b = nodeById(flow, edge.to);
  const label: MessageKey = when === "fail" ? "flows.edge.titleBack" : when === "pass" ? "flows.edge.titlePass" : "flows.edge.titleNext";
  return (
    <>
      <PanelHead
        title={t(label)}
        subtitle={t("flows.edge.fromTo", { from: a ? text(a).name : from, to: b ? text(b).name : edge.to })}
        onClose={onClose}
      />
      <div className="flow-inspector__section">
        {when === "fail" ? (
          <Rounds flow={flow} from={from} rounds={edge.maxRounds} readOnly={readOnly} onChange={onChange} />
        ) : (
          <p className="flow-inspector__explain">{t("flows.edge.explain")}</p>
        )}
      </div>
      {!readOnly && (
        <div className="flow-inspector__footer">
          <Button
            variant="ghost"
            small
            className="btn--danger-text"
            onClick={() => {
              onChange(disconnect(flow, from, when));
              onClose();
            }}
          >
            {t("flows.edge.remove")}
          </Button>
        </div>
      )}
    </>
  );
}
