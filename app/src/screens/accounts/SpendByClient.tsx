import { useQuery } from "@tanstack/react-query";

import { useApi } from "../../api/context";
import type { Spend } from "../../api/types";
import { formatTokens } from "../../tasks/timeline";

export function formatUsd(cost: number | null): string {
  return cost === null ? "—" : `$${cost.toFixed(2).replace(".", ",")}`;
}

function Cells({ spend }: { spend: Spend }) {
  return (
    <>
      <td className="num">{formatTokens(spend.tokens)}</td>
      <td className="num">{formatUsd(spend.costUsd)}</td>
    </>
  );
}

/** Usage of the week per client and per project (spec phase 6). */
export function SpendByClient() {
  const api = useApi();
  const summary = useQuery({ queryKey: ["usageSummary", 7], queryFn: () => api.usageSummary(7) });
  const data = summary.data;
  return (
    <section className="card table-card" aria-labelledby="spend-title">
      <div className="section-head">
        <h2 id="spend-title">Uso de la semana por cliente</h2>
      </div>
      {!data || data.projects.length === 0 ? (
        <p className="empty">Sin uso registrado esta semana.</p>
      ) : (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>Cliente</th>
                <th>Proyectos</th>
                <th className="num">Tokens</th>
                <th className="num">Costo estimado</th>
              </tr>
            </thead>
            <tbody>
              {data.clients.map((c) => (
                <tr key={c.client ?? ""}>
                  <th scope="row">{c.client ?? <span className="muted">Sin cliente</span>}</th>
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
      <p className="small muted">
        Tokens que Hedy registró en sus tareas. El costo es una estimación: en suscripciones es lo que calcula la CLI (no
        lo que se cobra); en API keys, según el precio que pongas en Ajustes.
      </p>
    </section>
  );
}
