// The images of a user message in the timeline, loaded from the files Hedy
// kept, and its PDFs as named chips. A file deleted since then shows a quiet
// placeholder: the history still loads.

import { useQuery } from "@tanstack/react-query";

import { useApi } from "../../api/context";
import type { Attachment } from "../../api/types";
import { useT } from "../../i18n";
import { DocumentIcon, ImageIcon } from "./icons";

function MessagePdf({ image }: { image: Attachment }) {
  return (
    <span className="message-images__item message-images__item--file" title={image.name}>
      <DocumentIcon />
      <span className="message-images__name">{image.name}</span>
    </span>
  );
}

function MessageImage({ taskId, image }: { taskId: string; image: Attachment }) {
  const t = useT();
  const api = useApi();
  const src = useQuery({
    queryKey: ["attachment", taskId, image.file],
    queryFn: () => api.attachment(taskId, image.file),
    staleTime: Infinity,
  });
  if (src.isPending) return <span className="message-images__item message-images__item--loading" aria-hidden />;
  if (!src.data) {
    return (
      <span className="message-images__item message-images__item--missing" title={image.name}>
        <ImageIcon />
        <span className="sr-only">
          {image.name}: {t("composer.images.missing")}
        </span>
      </span>
    );
  }
  return (
    <span className="message-images__item" title={image.name}>
      <img src={src.data} alt={image.name} />
    </span>
  );
}

export function MessageImages({ taskId, images }: { taskId: string; images: Attachment[] }) {
  const t = useT();
  if (images.length === 0) return null;
  return (
    <ul className="message-images" aria-label={t("composer.images.inMessage")}>
      {images.map((image) => (
        <li key={image.file}>
          {image.mediaType === "application/pdf" ? (
            <MessagePdf image={image} />
          ) : (
            <MessageImage taskId={taskId} image={image} />
          )}
        </li>
      ))}
    </ul>
  );
}
