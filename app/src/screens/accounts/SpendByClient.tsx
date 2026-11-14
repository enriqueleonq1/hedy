import { useQuery } from "@tanstack/react-query";

import { useApi } from "../../api/context";
import type { Spend } from "../../api/types";
import { formatTokens, formatUsd } from "../../format";
import { useLanguage, useT } from "../../i18n";

function Cells({ spend }: { spend: Spend }) {
  const language = useLanguage();
  return (
    <>
      <td className="num">{formatTokens(spend.tokens)}</td>
      <td className="num">{formatUsd(spend.costUsd, language)}</td>
    </>
  );
}

/** Usage of the week per client and per project (spec phase 6). */
export function SpendByClient() {
  const api = useApi();
  const t = useT();
  const summary = useQuery({ queryKey: ["usageSummary", 7], queryFn: () => api.usageSummary(7) });
  const data = summary.data;
  return (
    <section className="card table-card" aria-labelledby="spend-title">
      <div className="section-head">
        <h2 id="spend-title">{t("accounts.spend.title")}</h2>
      </div>
      {!data || data.projects.length === 0 ? (
        <p className="empty">{t("common.noUsageThisWeek")}</p>
      ) : (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>{t("common.client")}</th>
                <th>{t("common.projects")}</th>
                <th className="num">Tokens</th>
                <th className="num">{t("accounts.spend.cost")}</th>
              </tr>
            </thead>
            <tbody>
              {data.clients.map((c) => (
                <tr key={c.client ?? ""}>
                  <th scope="row">{c.client ?? <span className="muted">{t("common.noClient")}</span>}</th>
                  <td className="muted">
                    {data.projects
                      .filter((p) => p.client === c.client)
                      .map((p) => `${p.name} (${formatTokens(p.tokens)})`)
                      .join(" · ")}
                  </td>
                  <Cells spend={c} />
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="small muted">{t("accounts.spend.note")}</p>
    </section>
  );
}
