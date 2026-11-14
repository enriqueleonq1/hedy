import { useState } from "react";

import { useAction } from "../api/context";
import type { CliProviderId, Overview, SettingKey, ThemeSetting } from "../api/types";
import { Mascot, MASCOT_STATE_LABEL, type MascotState } from "../components/Mascot/Mascot";
import { ColorPicker, Field, Segmented } from "../components/ui/controls";

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
          <Field key={api.id} label={api.name}>
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
      </section>

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
