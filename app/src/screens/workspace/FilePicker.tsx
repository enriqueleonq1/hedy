// The `@` picker above a project's composer: the files that match what
// follows the `@`. The text field keeps the focus and drives it with the
// arrow keys (`aria-activedescendant`), like the template picker.

import { useT } from "../../i18n";
import { splitPath } from "./mentions";

export function FilePicker({
  id,
  files,
  active,
  loading,
  optionId,
  onPick,
  onHover,
}: {
  id: string;
  files: string[];
  /** Index of the highlighted file. */
  active: number;
  loading: boolean;
  optionId: (index: number) => string;
  onPick: (path: string) => void;
  onHover: (index: number) => void;
}) {
  const t = useT();
  return (
    <div className="template-picker">
      {files.length > 0 ? (
        <ul className="template-picker__list" role="listbox" id={id} aria-label={t("composer.files.list")}>
          {files.map((path, index) => {
            const { name, dir } = splitPath(path);
            return (
              <li
                key={path}
                id={optionId(index)}
                role="option"
                aria-selected={index === active}
                aria-label={path}
                className={`template-option file-option${index === active ? " template-option--active" : ""}`}
                onMouseDown={(e) => {
                  e.preventDefault();
                  onPick(path);
                }}
                onMouseEnter={() => onHover(index)}
              >
                <span className="file-option__name">{name}</span>
                {dir && <span className="file-option__dir">{dir}</span>}
              </li>
            );
          })}
        </ul>
      ) : (
        <p className="template-picker__empty" role="status">
          {loading ? t("composer.files.loading") : t("composer.files.none")}
        </p>
      )}
      <p className="template-picker__hint" aria-hidden>
        {t("composer.files.hint")}
      </p>
    </div>
  );
}
