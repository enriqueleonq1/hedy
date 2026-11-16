// One place that starts things, used by home, the sidebar and the command
// palette: a project from scratch, an existing folder, or a quick question
// that belongs to no project.

import { createContext, useContext, useMemo, useState, type ReactNode } from "react";

import { useApi } from "../../api/context";
import type { Overview } from "../../api/types";
import type { View } from "../../components/Layout";
import type { MenuItem } from "../../components/ui/Menu";
import { useToast } from "../../components/ui/Toast";
import { useT } from "../../i18n";
import { AddProjectDialog, useAddProject } from "../accounts/AddProjectDialog";
import { NewProjectDialog } from "./NewProjectDialog";

interface Create {
  /**
   * Opens "Nuevo proyecto"; `name` starts its name field, `carry` continues
   * that conversation in the new project's first task (named after it unless
   * `name` says otherwise).
   */
  newProject: (options?: { name?: string; carry?: string }) => void;
  openFolder: () => void;
  /** Opens a new conversation; `draft` starts its composer with that text. */
  quickQuestion: (draft?: string) => void;
  /** The three, with a line each on what they are for. */
  menu: MenuItem[];
}

const CreateContext = createContext<Create | null>(null);

export function useCreate(): Create {
  const create = useContext(CreateContext);
  if (!create) throw new Error("useCreate must be used inside <CreateProvider>");
  return create;
}

export function CreateProvider({
  overview,
  onNavigate,
  children,
}: {
  overview: Overview;
  onNavigate: (view: View) => void;
  children: ReactNode;
}) {
  const t = useT();
  const toast = useToast();
  const api = useApi();
  const adding = useAddProject();
  const [creating, setCreating] = useState<{ name?: string; carry?: string } | null>(null);

  const value = useMemo<Create>(() => {
    const newProject = (options?: { name?: string; carry?: string }) => {
      const carry = options?.carry;
      if (!carry || options?.name) return setCreating(options ?? {});
      // The name the backend suggests from the conversation's title.
      void api
        .conversationContext(carry)
        .then((context) => setCreating({ carry, name: context.suggestedName }))
        .catch(() => setCreating({ carry }));
    };
    const openFolder = () => void adding.start();
    const quickQuestion = (draft?: string) => onNavigate({ screen: "conversation", draft });
    return {
      newProject,
      openFolder,
      quickQuestion,
      menu: [
        { label: t("create.newProject"), hint: t("create.newProjectHint"), onSelect: () => newProject() },
        { label: t("create.openFolder"), hint: t("create.openFolderHint"), onSelect: openFolder },
        { label: t("create.quickQuestion"), hint: t("create.quickQuestionHint"), onSelect: () => quickQuestion() },
      ],
    };
  }, [adding, api, onNavigate, t]);

  return (
    <CreateContext.Provider value={value}>
      {children}
      {creating && (
        <NewProjectDialog
          overview={overview}
          initialName={creating.name}
          onClose={() => setCreating(null)}
          onCreated={(path, name) => {
            const carry = creating.carry;
            setCreating(null);
            toast.info(t("newProject.created", { name }));
            // Straight to the composer of its first task, focused (with the conversation, if carried).
            onNavigate({ screen: "workspace", project: path, task: null, ...(carry ? { carry } : {}) });
          }}
        />
      )}
      {adding.path && (
        <AddProjectDialog
          overview={overview}
          path={adding.path}
          onClose={adding.close}
          onAdded={(path) => onNavigate({ screen: "workspace", project: path, task: null })}
        />
      )}
    </CreateContext.Provider>
  );
}
