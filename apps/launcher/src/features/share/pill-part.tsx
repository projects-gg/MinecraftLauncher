// Screen sharing in the title bar's room pill (SHARE): while the player shares, a calm mark and
// how many watch (the panel on a press); otherwise, where the room shares, "Ekranını paylaş" as one
// more round button. Never red, never blinking.
import { Monitor, Screencast } from "@phosphor-icons/react";
import { cn, Tooltip } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { useVoiceUi } from "@/features/voice/store";
import { useShare, useShareUi } from "./store";

const round =
  "relative flex h-[22px] cursor-pointer items-center justify-center gap-1 rounded-full outline-none " +
  "transition-[background-color,color,transform,scale] duration-150 ease-out active:scale-[0.92] " +
  "focus-visible:ring-2 focus-visible:ring-gold-300 after:absolute after:-inset-x-0.5 after:-inset-y-1.5";

export function SharePillPart() {
  const { t } = useTranslation();
  const share = useShare();
  const openPicker = useShareUi((s) => s.openPicker);
  const openPanel = useVoiceUi((s) => s.openPanel);
  if (!share.on) return null;
  if (share.mine.length) {
    const count = new Set(share.mine.flatMap((m) => m.watchers)).size;
    const label = count ? t("share.live.pill", { count }) : t("share.live.pillAlone");
    return (
      <Tooltip content={label}>
        <button
          type="button"
          aria-label={label}
          onClick={(event) => openPanel({ from: event.currentTarget })}
          className={cn(round, "px-1.5 text-jade-400 hover:bg-white/10")}
        >
          <Screencast size={15} weight="bold" aria-hidden />
          {count ? (
            <span className="text-[12px] leading-none font-semibold tabular-nums text-bone-100">
              {count}
            </span>
          ) : null}
        </button>
      </Tooltip>
    );
  }
  if (!share.canShare || !share.allowed || !share.open) return null;
  return (
    <Tooltip content={t("share.action")}>
      <button
        type="button"
        aria-label={t("share.action")}
        onClick={openPicker}
        className={cn(round, "size-[22px] text-ash-300 hover:bg-white/10 hover:text-bone-50")}
      >
        <Monitor size={15} weight="bold" aria-hidden />
      </button>
    </Tooltip>
  );
}
