import { useMemo, useState } from "react";

import { errorMessage } from "../../api/context";
import type { Overview, Project, SkillOverview, SkillVerdict } from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { Button } from "../../components/ui/Button";
import { useT } from "../../i18n";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { ExportDialog, ImportDialog } from "./SkillDialogs";
import { SkillDetail } from "./SkillDetail";
import { TrialDialog } from "./TrialDialog";
import { useSkillsOverview, verdictKey } from "./library";

type Filter = "all" | "global" | "project" | "proposed";

/** The verdict a skill shows: its trial's, else a warning the analysis suggests, else untested. */
export function shownVerdict(item: SkillOverview): SkillVerdict {
  if (item.trial) return item.trial.verdict;
  return item.verdict === "useful" ? "untested" : item.verdict;
}

/** Where a skill is edited, reviewed and tried from: its project, or one that sees global skills. */
export function homeProject(item: SkillOverview, projects: Project[]): string | null {
  return item.skill.project ?? projects.find((p) => !p.protected)?.path ?? null;
}

/**
 * Every skill in one place: the global ones and each project's, with a search,
 * filters and the selected one in detail (what it costs, what Hedy measures,
 * its trial, "Usar también fuera de Hedy" and its steps).
 */
export function SkillsScreen({ overview }: { overview: Overview }) {
  const t = useT();
  const skills = useSkillsOverview();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [selected, setSelected] = useState<number | null>(null);
  const [dialog, setDialog] = useState<"import" | "export" | null>(null);
  const [trying, setTrying] = useState<SkillOverview | null>(null);

  const all = useMemo(() => skills.data ?? [], [skills.data]);
  const counts = {
    all: all.length,
    global: all.filter((s) => s.skill.project === null).length,
    project: all.filter((s) => s.skill.project !== null).length,
    proposed: all.filter((s) => s.skill.status === "proposed").length,
  };
  const needle = query.trim().toLowerCase();
  const shown = all.filter((s) => {
    const inFilter =
      filter === "all" ||
      (filter === "global" && s.skill.project === null) ||
      (filter === "project" && s.skill.project !== null) ||
      (filter === "proposed" && s.skill.status === "proposed");
    const matches =
      needle === "" ||
      s.skill.name.toLowerCase().includes(needle) ||
      s.skill.description.toLowerCase().includes(needle) ||
      (s.projectName ?? "").toLowerCase().includes(needle);
    return inFilter && matches;
  });
  const current = shown.find((s) => s.skill.id === selected) ?? shown[0] ?? null;
  const color = overview.accounts.find((a) => a.id === overview.defaultAccount)?.color ?? DEFAULT_ACCOUNT_COLOR;

  return (
    <div className="screen skill-lib">
      <header className="skill-lib__head">
        <div className="skill-lib__titles">
          <h1>{t("skillLib.title")}</h1>
          <p className="muted">{t("skillLib.subtitle")}</p>
        </div>
        <div className="skill-lib__actions">
          <Button onClick={() => setDialog("export")}>{t("skillLib.export")}</Button>
          <Button variant="primary" disabled={overview.projects.length === 0} onClick={() => setDialog("import")}>
            {t("skillLib.import")}
          </Button>
        </div>
      </header>

      <div className="skill-lib__toolbar">
        <input
          type="search"
          className="skill-lib__search"
          aria-label={t("skillLib.search")}
          placeholder={t("skillLib.search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <div className="skill-lib__filters" role="group" aria-label={t("skillLib.filters")}>
          {(["all", "global", "project", "proposed"] as const).map((f) => (
            <button
              key={f}
              type="button"
              aria-pressed={filter === f}
              className="skill-lib__filter"
              onClick={() => setFilter(f)}
            >
              {t(`skillLib.filter.${f}`, { n: counts[f] })}
            </button>
          ))}
        </div>
      </div>

      {skills.isPending ? (
        <p className="muted skill-lib__state">{t("skillLib.loading")}</p>
      ) : skills.isError ? (
        <p role="alert" className="skill-lib__state">
          {t("skillLib.loadError", { error: errorMessage(skills.error, t) })}
        </p>
      ) : all.length === 0 ? (
        <div className="skill-lib__empty">
          <Mascot color={color} state="sleeping" size={140} />
          <p className="skills__empty-title">{t("skillLib.empty")}</p>
          <p className="hint">{t("skillLib.emptyHint")}</p>
        </div>
      ) : (
        <div className="skill-lib__body">
          <ul className="skill-lib__list" aria-label={t("skillLib.list")}>
            {shown.length === 0 && <li className="muted skill-lib__none">{t("skillLib.noMatch")}</li>}
            {shown.map((item) => (
              <li key={item.skill.id}>
                <SkillRow item={item} selected={item.skill.id === current?.skill.id} onSelect={() => setSelected(item.skill.id)} />
              </li>
            ))}
          </ul>
          {current ? (
            <SkillDetail
              key={current.skill.id}
              item={current}
              projects={overview.projects}
              onTry={() => setTrying(current)}
            />
          ) : (
            <p className="muted skill-lib__pick">{t("skillLib.pick")}</p>
          )}
        </div>
      )}

      {dialog === "import" && <ImportDialog projects={overview.projects} onClose={() => setDialog(null)} />}
      {dialog === "export" && <ExportDialog skills={all} onClose={() => setDialog(null)} />}
      {trying && <TrialDialog item={trying} projects={overview.projects} onClose={() => setTrying(null)} />}
    </div>
  );
}

function SkillRow({ item, selected, onSelect }: { item: SkillOverview; selected: boolean; onSelect: () => void }) {
  const t = useT();
  const verdict = shownVerdict(item);
  const meta = [
    item.projectName ?? t("skillLib.meta.global"),
    item.skill.status === "proposed" ? t("skillLib.meta.proposed") : item.skill.author === "user" ? t("skillLib.meta.you") : item.skill.author,
    item.outside ? t("skillLib.meta.outside") : t("skillLib.meta.tokens", { n: item.tokens.toLocaleString() }),
  ];
  return (
    <button type="button" className="skill-row" aria-current={selected ? "true" : undefined} onClick={onSelect}>
      <span className="skill-row__top">
        <span className="skill-row__name">{item.skill.name}</span>
        <span className={`verdict verdict--${verdict}`}>{t(verdictKey[verdict])}</span>
      </span>
      <span className="skill-row__description">{item.skill.description}</span>
      <span className="skill-row__meta">{meta.join(" · ")}</span>
    </button>
  );
}
