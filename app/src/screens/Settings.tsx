import { useState } from "react";

import { useAction } from "../api/context";
import { useTerminals } from "../components/Terminal/TerminalProvider";
import { Button } from "../components/ui/Button";
import type { CliProviderId, Overview, SettingKey, ThemeSetting } from "../api/types";
import { Mascot, MASCOT_STATE_LABEL, type MascotState } from "../components/Mascot/Mascot";
import { ColorPicker, Field, Segmented, Switch } from "../components/ui/controls";

const CLIS: { id: CliProviderId; name: string }[] = [
  { id: "claude", name: "Claude Code" },
  { id: "codex", name: "Codex" },
  { id: "gemini", name: "Gemini CLI" },
];

export function Settings({ overview }: { overview: Overview }) {
  const setSetting = useAction((api, vars: { key: SettingKey; value: string | null }) =>
    api.setSetting(vars.key, vars.value),
  );
  const setColor = useAction((api, vars: { id: string; color: string }) => api.setAccountColor(vars.id, vars.color));
  const [preview, setPreview] = useState(overview.accounts[0]?.color ?? "#7AA2F7");

  return (
    <div className="screen settings">
      <h1>Ajustes</h1>

      <section className="card settings__section">
        <h2>Apariencia</h2>
        <div className="field">
          <span className="field__label">Tema</span>
          <Segmented<ThemeSetting>
            label="Tema"
            value={overview.theme}
            onChange={(value) => setSetting.mutate({ key: "ui.theme", value })}
            options={[
              { value: "system", label: "Según el sistema" },
              { value: "light", label: "Claro" },
              { value: "dark", label: "Oscuro" },
            ]}
          />
        </div>
        <Field label="Tu nombre" hint="Para saludarte en la bienvenida.">
          {(id) => (
            <input
              id={id}
              key={overview.userName}
              defaultValue={overview.userName}
              onBlur={(e) =>
                e.target.value.trim() !== overview.userName &&
                setSetting.mutate({ key: "ui.name", value: e.target.value })
              }
            />
          )}
        </Field>
      </section>

      <section className="card settings__section">
        <h2>Colores de cuenta</h2>
        {overview.accounts.length === 0 && <p className="hint">Aún no hay cuentas.</p>}
        {overview.accounts.map((a) => (
          <div key={a.id} className="row row--between settings__color-row">
            <span>{a.name}</span>
            <ColorPicker
              label={`Color de ${a.name}`}
              value={a.color}
              onChange={(color) => setColor.mutate({ id: a.id, color })}
            />
          </div>
        ))}
        <div className="mascot-gallery" aria-label="Estados de la mascota">
          {(Object.keys(MASCOT_STATE_LABEL) as MascotState[]).map((state) => (
            <figure key={state}>
              <Mascot color={preview} state={state} size={110} />
              <figcaption>{MASCOT_STATE_LABEL[state]}</figcaption>
            </figure>
          ))}
        </div>
        <div className="field">
          <span className="field__label">Probar color</span>
          <ColorPicker label="Color de prueba de la mascota" value={preview} onChange={setPreview} />
        </div>
      </section>

      <section className="card settings__section">
        <h2>Rutas de las CLIs</h2>
        <p className="hint">Déjalo vacío si el comando está en tu PATH.</p>
        {CLIS.map((cli) => (
          <Field key={cli.id} label={cli.name}>
            {(id) => (
              <input
                id={id}
                className="mono"
                key={overview.binaries[cli.id] ?? ""}
                defaultValue={overview.binaries[cli.id] ?? ""}
                placeholder={cli.id}
                spellCheck={false}
                onBlur={(e) =>
                  e.target.value.trim() !== (overview.binaries[cli.id] ?? "") &&
                  setSetting.mutate({ key: `binary.${cli.id}`, value: e.target.value })
                }
              />
            )}
          </Field>
        ))}
      </section>

      <section className="card settings__section">
        <h2>APIs</h2>
        <p className="hint">Déjalo vacío para usar la dirección oficial. Kimi Code o la región de China tienen las suyas.</p>
        {(
          [
            { id: "kimi", name: "Kimi", placeholder: "https://api.moonshot.ai/v1" },
            { id: "gemini", name: "Gemini", placeholder: "https://generativelanguage.googleapis.com/v1beta/openai" },
          ] as const
        ).map((api) => (
          <Field key={api.id} label={`${api.name}: dirección`}>
            {(id) => (
              <input
                id={id}
                className="mono"
                key={overview.apiUrls[api.id] ?? ""}
                defaultValue={overview.apiUrls[api.id] ?? ""}
                placeholder={api.placeholder}
                spellCheck={false}
                onBlur={(e) =>
                  e.target.value.trim() !== (overview.apiUrls[api.id] ?? "") &&
                  setSetting.mutate({ key: `api_url.${api.id}`, value: e.target.value })
                }
              />
            )}
          </Field>
        ))}
        <p className="hint">
          Precio para estimar el costo de las tareas por API: entrada/salida en USD por millón de tokens, como lo publica
          el proveedor (por ejemplo, 0.60/2.50). Sin precio, Hedy solo cuenta tokens.
        </p>
        {(["kimi", "gemini"] as const).map((provider) => (
          <Field key={provider} label={`${provider === "kimi" ? "Kimi" : "Gemini"}: precio`}>
            {(id) => (
              <input
                id={id}
                className="mono"
                key={overview.apiPrices[provider] ?? ""}
                defaultValue={overview.apiPrices[provider] ?? ""}
                placeholder="entrada/salida"
                spellCheck={false}
                onBlur={(e) =>
                  e.target.value.trim() !== (overview.apiPrices[provider] ?? "") &&
                  setSetting.mutate({ key: `api_price.${provider}`, value: e.target.value.trim() || null })
                }
              />
            )}
          </Field>
        ))}
      </section>

      <CodeMap overview={overview} />

      <section className="card settings__section">
        <h2>Datos</h2>
        <p>
          Hedy guarda todo en tu equipo: <span className="mono">{overview.dataDir}</span>
        </p>
        <p className="hint">
          Para usar otra carpeta, abre Hedy con la variable <span className="mono">HEDY_DATA_DIR</span>. Los logins de
          suscripción viven en la carpeta oficial de cada cuenta; las API keys, en el llavero del sistema.
        </p>
      </section>
    </div>
  );
}

/**
 * The code map (codebase-memory-mcp): a local index of the code that every AI
 * can query. Hedy installs it with its official script and registers it per
 * run; the index stays on this PC.
 */
function CodeMap({ overview }: { overview: Overview }) {
  const terminals = useTerminals();
  const setSetting = useAction((api, value: string | null) => api.setSetting("code_map.binary", value));
  const { enabled, found } = overview.codeMap;
  return (
    <section className="card settings__section">
      <h2>Mapa del código</h2>
      <p className="hint">
        Un índice local del código (funciones, llamadas, rutas) que todas tus IAs pueden consultar para gastar menos
        tokens leyendo archivos. Usa <span className="mono">codebase-memory-mcp</span> (MIT, sin telemetría); el índice
        se guarda en tu PC.
      </p>
      {found ? (
        <div className="row row--between">
          <span>
            Instalado en <span className="mono">{found}</span>
          </span>
          <Switch
            label="Registrar el mapa del código en las tareas"
            checked={enabled !== null}
            disabled={setSetting.isPending}
            onChange={(on) => setSetting.mutate(on ? found : null)}
          />
        </div>
      ) : (
        <div className="row row--between">
          <span className="muted">
            {enabled ? `Configurado como ${enabled}, pero no lo encuentro.` : "No está instalado."}
          </span>
          <Button onClick={() => terminals.open({ kind: "installCodeMap" }, "Instalar mapa del código")}>
            Instalar
          </Button>
        </div>
      )}
      <p className="small muted">
        Se instala con el script oficial, que verifica la descarga, y sin tocar la configuración de tus CLIs: Hedy lo
        registra en cada tarea, limitado a la carpeta del proyecto. Al terminar, vuelve a esta pantalla para activarlo.
      </p>
    </section>
  );
}
