// What a step is called and the one line under it, in the user's language.

import { useCallback } from "react";

import type { FlowNode, RoleDef, Seat } from "../../api/types";
import { useT } from "../../i18n";

export interface StepText {
  name: string;
  /** The model, the command, the trigger…; empty when there is nothing to add. */
  detail: string;
}

export function useStepText(roles: RoleDef[]): (node: FlowNode) => StepText {
  const t = useT();
  return useCallback(
    (node: FlowNode): StepText => {
      const name = (s: Seat) => roles.find((r) => r.id === s.role)?.name ?? s.role;
      switch (node.kind) {
        case "start":
          return { name: t("flows.step.start"), detail: "" };
        case "end":
          return { name: t("flows.step.end"), detail: t("flows.step.endDetail") };
        case "approval":
          return { name: t("flows.step.approval"), detail: t("flows.step.approvalDetail") };
        case "role":
          return { name: name(node), detail: node.model ?? "" };
        case "parallel":
          return { name: node.roles.map(name).join(" + "), detail: t("flows.step.parallelDetail") };
        case "condition":
          return node.check === "tests"
            ? { name: t("flows.step.tests"), detail: node.command ?? t("flows.step.testsDetail") }
            : node.check === "rules"
              ? { name: t("flows.step.rules"), detail: "" }
              : { name: t("flows.step.verdict"), detail: "" };
      }
    },
    [roles, t],
  );
}
