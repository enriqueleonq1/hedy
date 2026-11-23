// Images and PDFs in the composer: picked with the paperclip, pasted or
// dropped, shown as small removable thumbnails (a named chip for a PDF) above
// the text, and sent with the message. The backend checks them again (type
// from the bytes, size, count).

import { useId, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { useApi } from "../../api/context";
import type { ImageInput, ImageMediaType, ImageSupport } from "../../api/types";
import { useToast } from "../../components/ui/Toast";
import { useT, type MessageKey } from "../../i18n";
import { CloseIcon, DocumentIcon, PaperclipIcon } from "./icons";

export const IMAGE_TYPES: ImageMediaType[] = ["image/png", "image/jpeg", "image/gif", "image/webp"];
export const PDF_TYPE: ImageMediaType = "application/pdf";
export const MAX_IMAGE_BYTES = 5 * 1024 * 1024;
export const MAX_IMAGES = 4;

/** An image or PDF waiting in the composer. */
export interface DraftImage {
  id: string;
  name: string;
  pdf: boolean;
  /** Base64, as the backend takes it. */
  data: string;
  /** A data URL, for the thumbnail. */
  url: string;
}

export type ImageProblem = { key: MessageKey; params?: Record<string, string | number> };

/**
 * Which of `files` fit next to `current` attachments, and why the others do
 * not. `pdfs`: whether the agent reads PDFs.
 */
export function sortImageFiles(
  files: File[],
  current: number,
  pdfs = false,
): { accepted: File[]; problems: ImageProblem[] } {
  const accepted: File[] = [];
  const problems: ImageProblem[] = [];
  for (const file of files) {
    const pdf = file.type === PDF_TYPE;
    if (pdf && !pdfs) {
      problems.push({ key: "composer.pdf.notAccepted", params: { name: file.name } });
    } else if (!pdf && !IMAGE_TYPES.includes(file.type as ImageMediaType)) {
      problems.push({ key: "composer.images.unsupported", params: { name: file.name } });
    } else if (file.size > MAX_IMAGE_BYTES) {
      problems.push({ key: "composer.images.tooLarge", params: { name: file.name } });
    } else if (current + accepted.length >= MAX_IMAGES) {
      if (!problems.some((p) => p.key === "composer.images.tooMany")) {
        problems.push({ key: "composer.images.tooMany", params: { max: MAX_IMAGES } });
      }
    } else {
      accepted.push(file);
    }
  }
  return { accepted, problems };
}

let nextDraft = 0;

/** Reads a file into a draft (base64 for the backend, a data URL for the thumbnail). */
export function readImage(file: File): Promise<DraftImage> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error(file.name));
    reader.onload = () => {
      const url = String(reader.result ?? "");
      resolve({
        id: `img-${++nextDraft}`,
        name: file.name || "image",
        pdf: file.type === PDF_TYPE,
        data: url.slice(url.indexOf(",") + 1),
        url,
      });
    };
    reader.readAsDataURL(file);
  });
}

/** The files of a paste or a drop. */
export function transferFiles(transfer: DataTransfer | null): File[] {
  if (!transfer) return [];
  if (transfer.files.length > 0) return [...transfer.files];
  return [...transfer.items].filter((item) => item.kind === "file").flatMap((item) => item.getAsFile() ?? []);
}

/** Whether a drag carries files (not text dragged inside the window). */
export function draggingFiles(transfer: DataTransfer | null): boolean {
  return !!transfer && [...transfer.types].includes("Files");
}

/**
 * The composer's images. `account` is who gets the message; while the
 * message would wait in the queue, images cannot be attached.
 */
export function useComposerImages({ account, queueing }: { account: string | null; queueing: boolean }) {
  const t = useT();
  const api = useApi();
  const toast = useToast();
  const [images, setImages] = useState<DraftImage[]>([]);
  const support = useQuery({
    queryKey: ["imageSupport", account],
    queryFn: () => api.imageSupport(account ?? ""),
    enabled: !!account,
    staleTime: 60_000,
  });
  const pdfSupport = useQuery({
    queryKey: ["pdfSupport", account],
    queryFn: () => api.pdfSupport(account ?? ""),
    enabled: !!account,
    staleTime: 60_000,
  });
  const supported: ImageSupport | undefined = account ? support.data : "no";
  /** Whether the agent reads PDFs (an ACP agent may still say no when it starts). */
  const pdfs = !!account && !!pdfSupport.data && pdfSupport.data !== "no";
  /** Why images cannot be attached now; `null` when they can. */
  const blocked: string | null =
    supported === "no" ? t("composer.images.notAccepted") : queueing ? t("composer.images.whileQueued") : null;

  async function add(files: File[]) {
    if (files.length === 0) return;
    if (blocked) {
      toast.info(blocked);
      return;
    }
    const { accepted, problems } = sortImageFiles(files, images.length, pdfs);
    for (const problem of problems) toast.error(t(problem.key, problem.params));
    const read = await Promise.all(
      accepted.map((file) =>
        readImage(file).catch(() => {
          toast.error(t("composer.images.unreadable", { name: file.name }));
          return null;
        }),
      ),
    );
    const drafts = read.filter((d): d is DraftImage => d !== null);
    if (drafts.length > 0) setImages((old) => [...old, ...drafts].slice(0, MAX_IMAGES));
  }

  return {
    images,
    supported,
    pdfs,
    blocked,
    add,
    remove: (id: string) => setImages((old) => old.filter((i) => i.id !== id)),
    clear: () => setImages([]),
    /** What goes to the backend. */
    inputs: (): ImageInput[] => images.map(({ name, data }) => ({ name, data })),
  };
}

/** The paperclip: picks images (and PDFs, when the agent reads them) from disk, or says why it cannot. */
export function AttachButton({
  blocked,
  supported,
  pdfs = false,
  disabled,
  onFiles,
}: {
  blocked: string | null;
  supported: ImageSupport | undefined;
  pdfs?: boolean;
  disabled: boolean;
  onFiles: (files: File[]) => void;
}) {
  const t = useT();
  const toast = useToast();
  const picker = useRef<HTMLInputElement>(null);
  const detailId = useId();
  const unavailable = disabled || !!blocked;
  const detail =
    blocked ??
    (supported === "ifAgentAllows"
      ? t("composer.images.maybe")
      : t(pdfs ? "composer.images.attachDetailPdf" : "composer.images.attachDetail"));
  return (
    <>
      {/* Not `disabled`: it stays focusable and its tooltip says why. */}
      <button
        type="button"
        className="chip chip--icon"
        aria-label={t("composer.images.attach")}
        aria-describedby={detailId}
        aria-disabled={unavailable || undefined}
        title={`${t("composer.images.attach")} · ${detail}`}
        onClick={() => {
          if (blocked) toast.info(blocked);
          else if (!disabled) picker.current?.click();
        }}
      >
        <PaperclipIcon />
      </button>
      <span id={detailId} hidden>
        {detail}
      </span>
      <input
        ref={picker}
        type="file"
        accept={(pdfs ? [...IMAGE_TYPES, PDF_TYPE] : IMAGE_TYPES).join(",")}
        multiple
        hidden
        tabIndex={-1}
        aria-hidden
        onChange={(e) => {
          onFiles([...(e.target.files ?? [])]);
          e.target.value = "";
        }}
      />
    </>
  );
}

/** The attached images and PDFs, above the text: each one removable with the mouse or the keyboard. */
export function ImageTray({
  images,
  onRemove,
  onEmptied,
}: {
  images: DraftImage[];
  onRemove: (id: string) => void;
  /** The last image went: the focus goes back to the text. */
  onEmptied: () => void;
}) {
  const t = useT();
  const list = useRef<HTMLUListElement>(null);
  if (images.length === 0) return null;

  function remove(index: number) {
    const image = images[index];
    if (!image) return;
    onRemove(image.id);
    // The focus moves to the neighbour that takes its place, or back to the text.
    requestAnimationFrame(() => {
      const buttons = list.current?.querySelectorAll<HTMLButtonElement>("button");
      const next = buttons?.[Math.min(index, buttons.length - 1)];
      if (next) next.focus();
      else onEmptied();
    });
  }

  return (
    <ul className="image-tray" aria-label={t("composer.images.list")} ref={list}>
      {images.map((image, index) => (
        <li key={image.id} className={image.pdf ? "image-tray__item image-tray__item--file" : "image-tray__item"}>
          {image.pdf ? (
            <span className="image-tray__file" title={image.name}>
              <DocumentIcon />
              <span className="image-tray__name">{image.name}</span>
            </span>
          ) : (
            <img src={image.url} alt={image.name} title={image.name} />
          )}
          <button
            type="button"
            className="image-tray__remove"
            aria-label={t("composer.images.remove", { name: image.name })}
            title={t("composer.images.remove", { name: image.name })}
            onClick={() => remove(index)}
            onKeyDown={(e) => {
              if (e.key === "Delete" || e.key === "Backspace") {
                e.preventDefault();
                remove(index);
              }
            }}
          >
            <CloseIcon />
          </button>
        </li>
      ))}
    </ul>
  );
}
