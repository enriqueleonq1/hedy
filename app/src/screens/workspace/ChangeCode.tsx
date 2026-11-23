// A question ("Solo preguntar", "Investigar") that goes on to change code in
// the same task (docs/decisions.md §55): the offer under its answer and the
// divider its history shows from then on.

import { Button } from "../../components/ui/Button";
import { CodeIcon } from "../../components/ui/icons";
import { useT } from "../../i18n";
import "./carry/carry.css";

/** Under a question's answer: the next step when what is wrong is now clear. */
export function ChangeCodeSuggestion({ onChange }: { onChange: () => void }) {
  const t = useT();
  return (
    <div
      className="carry-suggest"
      role="group"
      aria-label={t("changeCode.suggest")}
    >
      <span className="carry-suggest__label">{t("changeCode.suggest")}</span>
      <Button
        small
        variant="ghost"
        title={t("changeCode.actionHint")}
        onClick={onChange}
      >
        <CodeIcon />
        {t("changeCode.action")}
      </Button>
    </div>
  );
}

/** Where a question started changing code: what came before only asked. */
export function ChangesCodeDivider({ branch }: { branch: string }) {
  const t = useT();
  return (
    <p
      className="changes-code-divider"
      role="separator"
      aria-label={t("changeCode.divider", { branch })}
    >
      <span className="changes-code-divider__label">
        <CodeIcon />
        {t("changeCode.divider", { branch })}
      </span>
    </p>
  );
}
