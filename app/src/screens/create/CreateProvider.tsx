// One place that starts things, used by home, the sidebar and the command
// palette: a project from scratch, an existing folder, or a quick question
// that belongs to no project.

import { createContext, useContext, useMemo, useState, type ReactNode } from "react";

import type { Overview } from "../../api/types";
import type { View } from "../../components/Layout";
import type { MenuItem } from "../../components/ui/Menu";
import { useToast } from "../../components/ui/Toast";
import { useT } from "../../i18n";
import { AddProjectDialog, useAddProject } from "../accounts/AddProjectDialog";
import { NewProjectDialog } from "./NewProjectDialog";

interface Create {
  newProject: () => void;
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
  const adding = useAddProject();
  const [creating, setCreating] = useState(false);

  const value = useMemo<Create>(() => {
    const newProject = () => setCreating(true);
    const openFolder = () => void adding.start();
    const quickQuestion = (draft?: string) => onNavigate({ screen: "conversation", draft });
    return {
      newProject,
      openFolder,
      quickQuestion,
      menu: [
        { label: t("create.newProject"), hint: t("create.newProjectHint"), onSelect: newProject },
        { label: t("create.openFolder"), hint: t("create.openFolderHint"), onSelect: openFolder },
        { label: t("create.quickQuestion"), hint: t("create.quickQuestionHint"), onSelect: () => quickQuestion() },
      ],
    };
  }, [adding, onNavigate, t]);

  return (
    <CreateContext.Provider value={value}>
      {children}
      {creating && (
        <NewProjectDialog
          overview={overview}
          onClose={() => setCreating(false)}
          onCreated={(path, name) => {
            setCreating(false);
            toast.info(t("newProject.created", { name }));
            // Straight to the composer of its first task, focused.
            onNavigate({ screen: "workspace", project: path, task: null });
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
