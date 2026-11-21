// The flow as a numbered list, in the order it runs: the same flow read top
// to bottom (and a way through it for the keyboard and screen readers).

import type { Flow, FlowNode, RoleDef } from "../../api/types";
import { useT } from "../../i18n";
import { edgeFrom, nodeById, runOrder } from "./flowModel";
import { StepIcon, stepTint } from "./StepIcon";
import { useStepText } from "./stepText";

export function FlowList({ flow, roles, issues, onOpen }: { flow: Flow; roles: RoleDef[]; issues: Set<string>; onOpen: (node: string) => void }) {
  const t = useT();
  const text = useStepText(roles);
  const describe = (node: FlowNode): string => {
    if (node.kind === "role") return roles.find((r) => r.id === node.role)?.summary ?? "";
    if (node.kind === "condition") return t(node.check === "tests" ? "flows.check.testsDetail" : node.check === "rules" ? "flows.check.rulesDetail" : "flows.check.verdictDetail");
    if (node.kind === "parallel") return t("flows.parallel.hint");
    return t(`flows.explain.${node.kind}`);
  };
  return (
    <div className="flow-list">
      <ol className="flow-list__steps" aria-label={t("flows.list.label")}>
        {runOrder(flow).map((node, i) => {
          const back = edgeFrom(flow, node.id, "fail");
          const target = back ? nodeById(flow, back.to) : undefined;
          const s = text(node);
          return (
            <li key={node.id}>
              <button type="button" className="flow-list__step" data-issue={issues.has(node.id) || undefined} onClick={() => onOpen(node.id)}>
                <span className="flow-list__icon" style={{ ["--tint" as string]: stepTint(node, roles) }}>
                  <StepIcon node={node} roles={roles} size={18} />
                </span>
                <span className="flow-list__body">
                  <span className="flow-list__name">
                    <span className="flow-list__n">{i + 1}.</span> {s.name}
                  </span>
                  <span className="flow-list__what">{describe(node)}</span>
                  {target && (
                    <span className="flow-list__back" data-check={node.kind === "condition" ? node.check : undefined}>
                      {t(node.kind === "condition" && node.check === "verdict" ? "flows.list.changesBack" : "flows.list.failBack", {
                        to: text(target).name,
                        rounds: String(back?.maxRounds ?? "∞"),
                      })}
                    </span>
                  )}
                </span>
                {s.detail && <span className="flow-list__detail">{s.detail}</span>}
              </button>
            </li>
          );
        })}
      </ol>
    </div>
  );
}
