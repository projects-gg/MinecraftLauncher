// Screen sharing in the title bar's room pill (SHARE): while the player shares, a calm mark and
// how many watch (the panel on a press; SQOL: a right click, Shift+F10 or the menu key opens the
// share's own menu, the card's "⋯"); otherwise, where the room shares, "Ekranını paylaş" as one
// more round button. Never red, never blinking.
import { Monitor, Screencast } from "@phosphor-icons/react";
import { ContextMenu, cn, Tooltip } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { useVoiceUi } from "@/features/voice/store";
import { ownShareName, useOwnPillItems } from "./own-menu";
import { openNewShare } from "./own-store";
import { useShare } from "./store";

const round =
  "relative flex h-[22px] cursor-pointer items-center justify-center gap-1 rounded-full outline-none " +
  "transition-[background-color,color,transform,scale] duration-150 ease-out active:scale-[0.92] " +
  "focus-visible:ring-2 focus-visible:ring-gold-300 after:absolute after:-inset-x-0.5 after:-inset-y-1.5";

export function SharePillPart() {
  const { t } = useTranslation();
  const share = useShare();
  const openPanel = useVoiceUi((s) => s.openPanel);
  const items = useOwnPillItems();
  if (!share.on) return null;
  if (share.mine.length) {
    const count = new Set(share.mine.flatMap((m) => m.watchers)).size;
    const label = count ? t("share.live.pill", { count }) : t("share.live.pillAlone");
    // Whose rows they are: the share's name (as on its card), or "Paylaşımların" for two.
    const [only] = share.mine;
    const title =
      share.mine.length === 1 && only ? ownShareName(only, t) : t("share.own.menuTitleMany");
    return (
      <ContextMenu
        items={items}
        title={title}
        disabled={!items.length}
        wrapTrigger={(trigger) => <Tooltip content={label}>{trigger}</Tooltip>}
      >
        <button
          type="button"
          aria-label={label}
          aria-keyshortcuts={items.length ? "Shift+F10" : undefined}
          onClick={(event) => openPanel({ from: event.currentTarget })}
          className={cn(
            round,
            "px-1.5 text-jade-400 hover:bg-white/10 data-[state=open]:bg-white/10",
          )}
        >
          <Screencast size={15} weight="bold" aria-hidden />
          {count ? (
            <span className="text-[12px] leading-none font-semibold tabular-nums text-bone-100">
              {count}
            </span>
          ) : null}
        </button>
      </ContextMenu>
    );
  }
  if (!share.canShare || !share.allowed || !share.open) return null;
  return (
    <Tooltip content={t("share.action")}>
      <button
        type="button"
        aria-label={t("share.action")}
        onClick={openNewShare}
        className={cn(round, "size-[22px] text-ash-300 hover:bg-white/10 hover:text-bone-50")}
      >
        <Monitor size={15} weight="bold" aria-hidden />
      </button>
    </Tooltip>
  );
}
