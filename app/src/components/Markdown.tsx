import ReactMarkdown, { type Components } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";

import { useApi } from "../api/context";
import { useT } from "../i18n";

/**
 * An agent's answer as Markdown (GitHub flavour: tables, task lists,
 * strikethrough; single line breaks kept as the agent wrote them). Raw HTML is never rendered, remote images are never loaded
 * (everything stays local: they show as their link), and links open in the
 * browser through Hedy, never inside the app.
 */
export function Markdown({ text, className }: { text: string; className?: string }) {
  const api = useApi();
  const t = useT();

  const components: Components = {
    a: ({ href, children }) => {
      const url = href ?? "";
      if (!url.startsWith("https://")) return <span className="md__link md__link--plain">{children}</span>;
      return (
        <a
          href={url}
          className="md__link"
          title={url}
          onClick={(e) => {
            e.preventDefault();
            void api.openLink(url);
          }}
        >
          {children}
        </a>
      );
    },
    img: ({ alt, src }) => (
      <span className="md__image" title={typeof src === "string" ? src : undefined}>
        {t("common.image", { alt: alt || (typeof src === "string" ? src : "") })}
      </span>
    ),
    table: ({ children }) => (
      <div className="md__table">
        <table>{children}</table>
      </div>
    ),
    input: ({ checked }) => <input type="checkbox" checked={!!checked} readOnly disabled />,
  };

  return (
    <div className={["md", className].filter(Boolean).join(" ")}>
      <ReactMarkdown remarkPlugins={[remarkGfm, remarkBreaks]} components={components} skipHtml>
        {text}
      </ReactMarkdown>
    </div>
  );
}
