import { useTranslation } from "react-i18next";
import type { ScreenshotEntry } from "@/lib/api";
import { formatRelative } from "@/lib/format";

export function ScreenshotsTab({ shots }: { shots: ScreenshotEntry[] }) {
  const { t } = useTranslation();
  if (!shots.length) return <p className="text-ash-400">{t("instance.screenshots.empty")}</p>;
  return (
    <ul className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
      {shots.map((s) => (
        <li
          key={s.fileName}
          className="group relative overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-800"
        >
          <img
            src={s.url}
            alt={s.fileName}
            loading="lazy"
            className="aspect-video w-full object-cover transition-transform duration-500 ease-out group-hover:scale-[1.04]"
          />
          <span className="absolute inset-x-0 bottom-0 bg-[linear-gradient(to_top,rgb(10_9_12/0.85),transparent)] px-3 pt-6 pb-2 text-[12px] text-bone-100 opacity-0 transition-opacity group-hover:opacity-100">
            {formatRelative(s.takenAt)}
          </span>
        </li>
      ))}
    </ul>
  );
}
