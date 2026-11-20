import { useEffect, useRef, useState, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { useAction, useApi } from "../../api/context";
import type { DetectedCli, Overview, ProviderId } from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { TerminalPanel } from "../../components/Terminal/TerminalPanel";
import { useTerminals } from "../../components/Terminal/TerminalProvider";
import { Button } from "../../components/ui/Button";
import { Segmented } from "../../components/ui/controls";
import { useLanguage, useT, type Language, type MessageKey } from "../../i18n";
import { accountPalette, DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { AddProjectDialog, useAddProject } from "../accounts/AddProjectDialog";

const STEPS = 6;
const READY = STEPS - 1;
const VERSION = "0.1";

/** How each CLI is shown, its dot color and where to get it. */
const CLIS: Record<ProviderId, { name: string; color: string; install: string }> = {
  claude: {
    name: "Claude Code",
    color: accountPalette[1]?.hex ?? DEFAULT_ACCOUNT_COLOR,
    install: "https://github.com/anthropics/claude-code",
  },
  codex: {
    name: "ChatGPT · Codex",
    color: accountPalette[2]?.hex ?? DEFAULT_ACCOUNT_COLOR,
    install: "https://github.com/openai/codex",
  },
  kimi: {
    name: "Kimi Code",
    color: accountPalette[3]?.hex ?? DEFAULT_ACCOUNT_COLOR,
    install: "https://moonshotai.github.io/kimi-code/en/guides/getting-started.html",
  },
  antigravity: {
    name: "Antigravity",
    color: accountPalette[4]?.hex ?? DEFAULT_ACCOUNT_COLOR,
    install: "https://antigravity.google",
  },
  gemini: {
    name: "Gemini CLI",
    color: accountPalette[0]?.hex ?? DEFAULT_ACCOUNT_COLOR,
    install: "https://github.com/google-gemini/gemini-cli",
  },
};

/** The order of the list; Gemini CLI (business accounts only) shows when installed. */
const ORDER: ProviderId[] = ["claude", "codex", "kimi", "antigravity", "gemini"];

/**
 * The first-run guide: what Hedy is, the accounts it found, how it keeps the
 * user in control, the shared memory, and the first project.
 */
export function Onboarding({
  overview,
  onFinish,
}: {
  overview: Overview;
  /** Called once the guide is done; with the project the user just added, if any. */
  onFinish: (project?: string) => void;
}) {
  const [step, setStep] = useState(0);
  const heading = useRef<HTMLHeadingElement>(null);
  const finish = useAction((api) => api.setSetting("ui.onboarded", "true"));
  const done = (project?: string) => finish.mutate(undefined, { onSuccess: () => onFinish(project) });

  // Screen readers and keyboards land on the new step's title.
  useEffect(() => {
    if (step > 0) heading.current?.focus();
  }, [step]);

  const next = () => setStep((s) => Math.min(s + 1, READY));
  const back = () => setStep((s) => Math.max(s - 1, 0));

  return (
    <div className="onboarding">
      <div className="onboarding__page">
        {step === 0 && <WelcomeStep overview={overview} onStart={next} />}
        {step > 0 && step < READY && (
          <TourStep
            step={step}
            onBack={back}
            onNext={next}
            onSkip={() => setStep(READY)}
            illustration={STEP_ILLUSTRATIONS[step]}
          >
            {step === 1 && <ConceptText heading={heading} />}
            {step === 2 && <AccountsText heading={heading} overview={overview} />}
            {step === 3 && <SafetyText heading={heading} />}
            {step === 4 && <MemoryText heading={heading} />}
          </TourStep>
        )}
        {step === READY && (
          <ReadyStep heading={heading} overview={overview} onBack={back} onDone={done} busy={finish.isPending} />
        )}
      </div>
      <TerminalPanel />
    </div>
  );
}

const STEP_ILLUSTRATIONS: Record<number, () => ReactNode> = {
  1: () => <ConceptIllustration />,
  2: () => <AccountsIllustration />,
  3: () => <SafetyIllustration />,
  4: () => <MemoryIllustration />,
};

function Progress({ step }: { step: number }) {
  const t = useT();
  return (
    <div className="onboarding__dots" role="img" aria-label={t("onboarding.step", { current: step + 1, total: STEPS })}>
      {Array.from({ length: STEPS }, (_, i) => (
        <span key={i} className={`onboarding__dot${i === step ? " onboarding__dot--current" : ""}`} />
      ))}
    </div>
  );
}

function WelcomeStep({ overview, onStart }: { overview: Overview; onStart: () => void }) {
  const t = useT();
  const language = useLanguage();
  const setLanguage = useAction((api, value: Language) => api.setSetting("ui.language", value));
  const color = overview.accounts[0]?.color ?? DEFAULT_ACCOUNT_COLOR;
  return (
    <section className="onboarding__welcome">
      <header className="onboarding__brand">
        <span className="onboarding__wordmark">
          <Mascot color={color} state="idle" size={30} label="" />
          Hedy
        </span>
        <span className="small muted">{t("onboarding.version", { version: VERSION })}</span>
      </header>
      <main className="onboarding__center">
        <div className="onboarding__enter">
          <Mascot color={color} state="idle" size={232} label={t("onboarding.mascotHello")} />
        </div>
        <div className="onboarding__intro">
          <h1>{t("onboarding.hello")}</h1>
          <p>{t("onboarding.tagline")}</p>
        </div>
        <div className="onboarding__language">
          <span className="onboarding__eyebrow-muted">{t("common.language")}</span>
          <Segmented<Language>
            label={t("common.language")}
            value={language}
            onChange={(value) => setLanguage.mutate(value)}
            options={[
              { value: "es", label: t("language.es") },
              { value: "en", label: t("language.en") },
            ]}
          />
        </div>
        <Button variant="primary" className="onboarding__cta" onClick={onStart}>
          {t("onboarding.start")}
        </Button>
      </main>
      <footer className="onboarding__privacy small muted">
        <LockIcon />
        {t("onboarding.privacy")}
      </footer>
    </section>
  );
}

function TourStep({
  step,
  illustration,
  children,
  onBack,
  onNext,
  onSkip,
}: {
  step: number;
  illustration: (() => ReactNode) | undefined;
  children: ReactNode;
  onBack: () => void;
  onNext: () => void;
  onSkip: () => void;
}) {
  const t = useT();
  return (
    <section className="onboarding__tour">
      <div className="onboarding__art">{illustration?.()}</div>
      <div className="onboarding__text">
        <header className="onboarding__skip">
          <button type="button" className="link-button" onClick={onSkip}>
            {t("common.skip")}
          </button>
        </header>
        <div className="onboarding__body">{children}</div>
        <footer className="onboarding__nav">
          <Progress step={step} />
          <div className="actions">
            <Button onClick={onBack}>{t("common.back")}</Button>
            <Button variant="primary" onClick={onNext}>
              {t("common.continue")}
            </Button>
          </div>
        </footer>
      </div>
    </section>
  );
}

function Title({
  heading,
  eyebrow,
  title,
}: {
  heading: React.RefObject<HTMLHeadingElement | null>;
  eyebrow: MessageKey;
  title: MessageKey;
}) {
  const t = useT();
  return (
    <div className="onboarding__title">
      <span className="onboarding__eyebrow">{t(eyebrow)}</span>
      <h1 ref={heading} tabIndex={-1}>
        {t(title)}
      </h1>
    </div>
  );
}

type HeadingRef = { heading: React.RefObject<HTMLHeadingElement | null> };

// ---------------------------------------------------------------- 2 · concept

function ConceptIllustration() {
  const t = useT();
  return (
    <div className="onboarding__stack">
      <div className="onboarding__card">
        <span className="onboarding__label">{t("onboarding.concept.accounts")}</span>
        <div className="pills">
          {(["claude", "codex", "antigravity", "kimi"] as ProviderId[]).map((p) => (
            <span key={p} className="chip">
              <span className="dot" style={{ background: CLIS[p].color }} aria-hidden />
              {p === "codex" ? "ChatGPT" : CLIS[p].name.replace(" Code", "")}
            </span>
          ))}
        </div>
      </div>
      <ArrowDown />
      <div className="onboarding__card onboarding__card--row">
        <div className="onboarding__project">
          <span className="onboarding__label">{t("onboarding.concept.project")}</span>
          <strong>{t("onboarding.concept.projectName")}</strong>
        </div>
        <span className="chip onboarding__chip--selected">
          <span className="dot" style={{ background: CLIS.claude.color }} aria-hidden />
          {t("onboarding.concept.accountChip")}
        </span>
      </div>
      <ArrowDown />
      <div className="onboarding__card">
        <span className="onboarding__label">{t("onboarding.concept.eachTask")}</span>
        <div className="onboarding__pipeline">
          <span>{t("onboarding.concept.plan")}</span>
          <span>{t("onboarding.concept.permissions")}</span>
          <span>{t("onboarding.concept.changes")}</span>
          <span className="onboarding__pipeline-last">{t("onboarding.concept.accept")}</span>
        </div>
      </div>
    </div>
  );
}

function ConceptText({ heading }: HeadingRef) {
  const t = useT();
  const items: [MessageKey, MessageKey][] = [
    ["onboarding.concept.step1", "onboarding.concept.step1Body"],
    ["onboarding.concept.step2", "onboarding.concept.step2Body"],
    ["onboarding.concept.step3", "onboarding.concept.step3Body"],
  ];
  return (
    <>
      <Title heading={heading} eyebrow="onboarding.concept.eyebrow" title="onboarding.concept.title" />
      <ol className="onboarding__steps">
        {items.map(([title, body], i) => (
          <li key={title}>
            <span className="onboarding__number">{i + 1}</span>
            <div>
              <strong>{t(title)}</strong>
              <p>{t(body)}</p>
            </div>
          </li>
        ))}
      </ol>
    </>
  );
}

// --------------------------------------------------------------- 3 · accounts

function AccountsIllustration() {
  const t = useT();
  return (
    <div className="onboarding__stack">
      <div className="onboarding__window">
        <div className="onboarding__window-bar">
          <span />
          <span />
          <span />
          <span className="small muted">{t("onboarding.accounts.loginWindow")}</span>
        </div>
        <div className="onboarding__signin">
          <span className="onboarding__signin-logo" style={{ background: CLIS.codex.color }} />
          <strong>{t("onboarding.accounts.continueWith")}</strong>
          <span className="onboarding__signin-field">{t("onboarding.accounts.email")}</span>
          <span className="onboarding__ok">{t("onboarding.accounts.loggedIn")}</span>
        </div>
      </div>
      <div className="onboarding__note">
        <span className="onboarding__icon">
          <ShieldIcon />
        </span>
        <p>{t("onboarding.accounts.privacy")}</p>
      </div>
    </div>
  );
}

function AccountsText({ heading, overview }: HeadingRef & { overview: Overview }) {
  const t = useT();
  const api = useApi();
  const client = useQueryClient();
  const terminals = useTerminals();
  const [opened, setOpened] = useState<string | null>(null);
  const detected = useQuery({ queryKey: ["detectClis"], queryFn: () => api.detectClis() });
  const connect = useAction(
    (a, cli: DetectedCli) =>
      a.addAccount({
        name: CLIS[cli.provider].name.replace("ChatGPT · ", ""),
        provider: cli.provider,
        apiKey: false,
        // The CLI's own folder: `claude`, `codex`… typed in a terminal keep working.
        defaultFolder: true,
        color: CLIS[cli.provider].color,
      }),
    {
      onSuccess: (id, cli) => {
        void client.invalidateQueries({ queryKey: ["detectClis"] });
        if (cli.login.state !== "loggedIn") {
          terminals.open({ kind: "login", account: id }, `Login · ${CLIS[cli.provider].name}`);
          setOpened(CLIS[cli.provider].name);
        }
      },
    },
  );
  const rows = ORDER.map((p) => detected.data?.find((d) => d.provider === p)).filter(
    (d): d is DetectedCli => !!d && (d.provider !== "gemini" || d.installed),
  );

  const detail = (d: DetectedCli): string => {
    if (!d.installed) return t("onboarding.accounts.notInstalled");
    if (d.login.state === "loggedIn") return t("onboarding.accounts.sessionFound");
    if (d.login.state === "loggedOut") return t("onboarding.accounts.installedNoSession");
    return t("onboarding.accounts.installed");
  };

  return (
    <>
      <Title heading={heading} eyebrow="onboarding.accounts.eyebrow" title="onboarding.accounts.title" />
      <p className="onboarding__lead">{t("onboarding.accounts.body")}</p>
      {detected.isPending && <p className="muted">{t("onboarding.accounts.searching")}</p>}
      <ul className="onboarding__clis">
        {rows.map((d) => {
          const connected = !!d.account || overview.accounts.some((a) => a.provider === d.provider && a.defaultFolder);
          return (
            <li key={d.provider}>
              <span className="dot onboarding__cli-dot" style={{ background: CLIS[d.provider].color }} aria-hidden />
              <div className="onboarding__cli-text">
                <strong>{CLIS[d.provider].name}</strong>
                <span className="small muted">{detail(d)}</span>
              </div>
              {connected ? (
                <span className="onboarding__ok">✓ {t("onboarding.accounts.connected")}</span>
              ) : d.installed ? (
                <Button
                  small
                  variant={d.login.state === "loggedIn" ? "primary" : "secondary"}
                  disabled={connect.isPending}
                  onClick={() => connect.mutate(d)}
                >
                  {d.login.state === "loggedIn" ? t("onboarding.accounts.useSession") : t("onboarding.accounts.logIn")}
                </Button>
              ) : (
                <button
                  type="button"
                  className="link-button"
                  onClick={() => void api.openLink(CLIS[d.provider].install)}
                >
                  {t("onboarding.accounts.howToInstall")}
                </button>
              )}
            </li>
          );
        })}
      </ul>
      {opened && (
        <p className="small muted" role="status">
          {t("onboarding.accounts.loginOpened", { name: opened })}
        </p>
      )}
    </>
  );
}

// ----------------------------------------------------------------- 4 · safety

function SafetyIllustration() {
  const t = useT();
  const [answer, setAnswer] = useState<"allow" | "reject" | null>(null);
  return (
    <div className="onboarding__stack">
      <div className="permission-card onboarding__permission">
        <h3>{t("onboarding.safety.cardTitle")}</h3>
        <code className="permission-card__what">{t("onboarding.safety.file")}</code>
        <div className="actions">
          <Button variant="primary" small onClick={() => setAnswer("allow")}>
            {t("onboarding.safety.allow")}
          </Button>
          <Button small onClick={() => setAnswer("allow")}>
            {t("onboarding.safety.allowChat")}
          </Button>
          <Button variant="ghost" small onClick={() => setAnswer("reject")}>
            {t("onboarding.safety.reject")}
          </Button>
        </div>
        {answer && (
          <p className="small" role="status">
            {answer === "allow" ? t("onboarding.safety.allowed") : t("onboarding.safety.rejected")}
          </p>
        )}
      </div>
      <div className="onboarding__diff">
        <div className="onboarding__diff-head">
          <strong>{t("onboarding.safety.file")}</strong>
          <span>
            <span className="plus">+1</span> <span className="minus">−1</span>
          </span>
        </div>
        <div className="diff__line">{"  "}{t("onboarding.safety.diffContext1")}</div>
        <div className="diff__line diff__line--removed">− {t("onboarding.safety.diffOld")}</div>
        <div className="diff__line diff__line--added">+ {t("onboarding.safety.diffNew")}</div>
        <div className="diff__line">{"  "}{t("onboarding.safety.diffContext2")}</div>
      </div>
    </div>
  );
}

function SafetyText({ heading }: HeadingRef) {
  const t = useT();
  const items: [ReactNode, MessageKey, MessageKey][] = [
    [<BranchIcon key="b" />, "onboarding.safety.branch", "onboarding.safety.branchBody"],
    [<CheckIcon key="c" />, "onboarding.safety.asks", "onboarding.safety.asksBody"],
    [<LockIcon key="l" />, "onboarding.safety.secrets", "onboarding.safety.secretsBody"],
    [<AlertIcon key="a" />, "onboarding.safety.limits", "onboarding.safety.limitsBody"],
  ];
  return (
    <>
      <Title heading={heading} eyebrow="onboarding.safety.eyebrow" title="onboarding.safety.title" />
      <ul className="onboarding__promises">
        {items.map(([icon, title, body]) => (
          <li key={title}>
            <span className="onboarding__promise-icon">{icon}</span>
            <span>
              <strong>{t(title)}</strong> <span className="muted">{t(body)}</span>
            </span>
          </li>
        ))}
      </ul>
    </>
  );
}

// ----------------------------------------------------------------- 5 · memory

function MemoryIllustration() {
  const t = useT();
  const [result, setResult] = useState<"saved" | "discarded" | null>(null);
  return (
    <div className="onboarding__stack onboarding__stack--notes">
      <SampleNote text="onboarding.memory.note1" category="decision" uses={23} />
      <SampleNote text="onboarding.memory.note2" category="convention" uses={31} />
      <div className="note note--proposed">
        <div className="note__head">
          <p className="note__text">{t("onboarding.memory.note3")}</p>
          <span className="category category--convention">{t("onboarding.memory.convention")}</span>
        </div>
        <div className="note__foot">
          <span className="small muted">
            <span className="note__author note__author--ai">Codex</span> {t("onboarding.memory.proposedBy")}
          </span>
          <span className="spacer" />
          {result === null ? (
            <>
              <Button small variant="primary" onClick={() => setResult("saved")}>
                {t("onboarding.memory.approve")}
              </Button>
              <Button small className="btn--danger" onClick={() => setResult("discarded")}>
                {t("onboarding.memory.reject")}
              </Button>
            </>
          ) : (
            <span className={result === "saved" ? "onboarding__ok" : "small muted"} role="status">
              {result === "saved" ? t("onboarding.memory.saved") : t("onboarding.memory.discarded")}
            </span>
          )}
        </div>
      </div>
      <div className="onboarding__readers small">
        <span className="muted">{t("onboarding.memory.readBy")}</span>
        {(["claude", "codex", "kimi"] as ProviderId[]).map((p) => (
          <span key={p} className="onboarding__reader">
            <span className="dot" style={{ background: CLIS[p].color }} aria-hidden />
            {p === "claude" ? "Claude" : p === "codex" ? "Codex" : "Kimi"}
          </span>
        ))}
      </div>
    </div>
  );
}

function SampleNote({ text, category, uses }: { text: MessageKey; category: "decision" | "convention"; uses: number }) {
  const t = useT();
  return (
    <div className="note">
      <div className="note__head">
        <p className="note__text">{t(text)}</p>
        <span className={`category category--${category}`}>
          {t(category === "decision" ? "onboarding.memory.decision" : "onboarding.memory.convention")}
        </span>
      </div>
      <span className="small muted">{t("onboarding.memory.byYou", { count: uses })}</span>
    </div>
  );
}

function MemoryText({ heading }: HeadingRef) {
  const t = useT();
  return (
    <>
      <Title heading={heading} eyebrow="onboarding.memory.eyebrow" title="onboarding.memory.title" />
      <p className="onboarding__lead">{t("onboarding.memory.body")}</p>
      <div className="onboarding__tip">
        <PenIcon />
        <span>{t("onboarding.memory.tip")}</span>
      </div>
    </>
  );
}

// ------------------------------------------------------------------ 6 · ready

function ReadyStep({
  heading,
  overview,
  onBack,
  onDone,
  busy,
}: HeadingRef & { overview: Overview; onBack: () => void; onDone: (project?: string) => void; busy: boolean }) {
  const t = useT();
  const adding = useAddProject();
  const color = overview.accounts[0]?.color ?? DEFAULT_ACCOUNT_COLOR;
  return (
    <section className="onboarding__welcome">
      <header className="onboarding__brand">
        <button type="button" className="link-button" onClick={onBack}>
          ← {t("common.back")}
        </button>
        <Progress step={READY} />
        <span className="onboarding__spacer" />
      </header>
      <main className="onboarding__center">
        <Mascot color={color} state="done" size={220} label={t("onboarding.mascotDone")} />
        <div className="onboarding__intro">
          <h1 ref={heading} tabIndex={-1}>
            {t("onboarding.ready.title")}
          </h1>
          <p>{t("onboarding.ready.body")}</p>
        </div>
        <div className="actions onboarding__ready-actions">
          <Button variant="primary" className="onboarding__cta" disabled={busy} onClick={() => void adding.start()}>
            <FolderIcon />
            {t("onboarding.ready.addProject")}
          </Button>
          <Button className="onboarding__cta" disabled={busy} onClick={() => onDone()}>
            {t("onboarding.ready.explore")}
          </Button>
        </div>
      </main>
      <footer className="onboarding__privacy small muted">{t("onboarding.ready.later")}</footer>
      {adding.path && (
        <AddProjectDialog
          overview={overview}
          path={adding.path}
          onClose={adding.close}
          onAdded={(path) => onDone(path)}
        />
      )}
    </section>
  );
}

// ------------------------------------------------------------------ icons

function Svg({ children }: { children: ReactNode }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width="20"
      height="20"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

const ArrowDown = () => (
  <span className="onboarding__arrow">
    <Svg>
      <path d="M12 5v14M6 13l6 6 6-6" />
    </Svg>
  </span>
);
const LockIcon = () => (
  <Svg>
    <rect x="4" y="11" width="16" height="10" rx="2" />
    <path d="M8 11V7a4 4 0 0 1 8 0v4" />
  </Svg>
);
const ShieldIcon = () => (
  <Svg>
    <path d="M12 3l7 3v5c0 5-3.5 8.5-7 10-3.5-1.5-7-5-7-10V6z" />
    <path d="M9 12l2 2 4-4" />
  </Svg>
);
const BranchIcon = () => (
  <Svg>
    <circle cx="6" cy="6" r="2" />
    <circle cx="6" cy="18" r="2" />
    <circle cx="18" cy="9" r="2" />
    <path d="M6 8v8M18 11c0 4-6 3-11 6" />
  </Svg>
);
const CheckIcon = () => (
  <Svg>
    <path d="M9 11l3 3 8-8" />
    <path d="M20 12v6a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h9" />
  </Svg>
);
const AlertIcon = () => (
  <Svg>
    <path d="M12 9v4M12 17h.01" />
    <path d="M10.3 3.9L2 18a2 2 0 0 0 1.7 3h16.6a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z" />
  </Svg>
);
const PenIcon = () => (
  <Svg>
    <path d="M12 20h9" />
    <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z" />
  </Svg>
);
const FolderIcon = () => (
  <Svg>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    <path d="M12 11v5M9.5 13.5h5" />
  </Svg>
);
