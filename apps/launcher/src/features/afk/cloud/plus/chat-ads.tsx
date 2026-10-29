// "Sohbete ilan" (TRADE-ANY, the owner, 28 Oct 2026: "probot ai kullanıcı isterse chatten
// kullanıcının ne aradığını da arada yazabilir … 30dk … birebir aynı mesaj yazılırsa sunucu taraflı
// spama da takılabilir"): under the switch, how often ProBot AI posts (30 or 45 minutes, an hour,
// two; the ones under the server's floor wait, said once) and its next ad as the game's chat shows
// it, the bot's name before the text, "Başka örnek" going through the next few. The service plans
// them (`adPreview`, the texts only, no price): a spam warning, today's stop, item names not known
// yet and no active order are its word. An older service plans nothing: none of this shows and the
// switch stays as it was.
import { cn } from "@pl/ui";
import { RadioGroup } from "radix-ui";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Hint } from "../../settings/controls";
import { GameChat } from "../setup/game-chat";
import { QuietLink } from "./fields";
import { AD_EVERY_DEFAULT, AD_EVERY_MIN } from "./model";
import type { PlusAdPreview } from "./types";

/** A pick of the minutes: the chosen one on the quiet plate (the gold is the save's). */
const CHIP = cn(
  "chamfer h-[28px] min-w-[56px] cursor-pointer px-3 text-[12px] font-semibold [--cut:5px]",
  "outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
  "transition-[background-color,color,opacity,scale] duration-150 ease-out active:scale-[0.96]",
  "data-[state=unchecked]:bg-white/[0.06] data-[state=unchecked]:text-ash-300",
  "data-[state=unchecked]:hover:bg-white/10 data-[state=unchecked]:hover:text-bone-100",
  "data-[state=checked]:bg-[color:var(--segmented-plate)] data-[state=checked]:text-bone-50",
  "disabled:cursor-not-allowed disabled:opacity-45 disabled:active:scale-100",
);

export function ChatAds({
  plan,
  minutes,
  name,
  onMinutes,
}: {
  /** The service's plan for the stored settings; null from an older service. */
  plan: PlusAdPreview | null;
  /** The player's pick as the draft has it; missing or null while none was picked. */
  minutes: number | null | undefined;
  /** The bot's in-game name, as the game's chat writes it before a line. */
  name: string;
  onMinutes: (minutes: number) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  const [sample, setSample] = useState(0);
  if (!plan) return null;
  const picked = minutes ?? AD_EVERY_DEFAULT;
  const floor = plan.floorMin;
  const floored = AD_EVERY_MIN.some((choice) => choice < floor);
  const count = plan.lines.length;
  const line = plan.lines[sample % Math.max(count, 1)] ?? null;
  // No line: the item names not known yet (the machine not in game), or no active order.
  const quiet = plan.namesPending ? "adNamesPending" : "adNoOrders";
  return (
    <div className="flex flex-col gap-1.5 py-2.5">
      <span id={`${id}-label`} className="text-[13px] font-medium text-bone-100">
        {t("afkPlus.settings.adEvery")}
      </span>
      <RadioGroup.Root
        aria-labelledby={`${id}-label`}
        aria-describedby={floored ? `${id}-floor` : undefined}
        orientation="horizontal"
        value={String(picked)}
        onValueChange={(value) => onMinutes(Number(value))}
        className="flex flex-wrap gap-1"
      >
        {AD_EVERY_MIN.map((choice) => (
          <RadioGroup.Item
            key={choice}
            value={String(choice)}
            disabled={choice < floor}
            className={CHIP}
          >
            {t(`afkPlus.settings.adEveryChoices.m${choice}`)}
          </RadioGroup.Item>
        ))}
      </RadioGroup.Root>
      {floored ? (
        <p id={`${id}-floor`} className="text-[12px] text-ash-400">
          {t("afkPlus.settings.adFloor", { n: floor })}
        </p>
      ) : null}
      {plan.stoppedToday ? (
        <Hint tone="warn" className="pt-1.5">
          {t("afkPlus.settings.adStopped")}
        </Hint>
      ) : (
        <>
          {plan.warning ? (
            <Hint tone="warn" className="pt-1.5">
              {t("afkPlus.settings.adWarned", { n: plan.warning.everyMin })}
            </Hint>
          ) : null}
          {line === null ? (
            <p className="pt-1.5 text-[12.5px] text-ash-400">{t(`afkPlus.settings.${quiet}`)}</p>
          ) : (
            <div className="flex w-full flex-col items-start gap-1 pt-1.5">
              {/* A line of the game's public chat in its own letters (SETUP-UX-L, the setup's
                  GameChat): the bot's name, then the text. Read once, in words. */}
              <p aria-live="polite" className="sr-only">
                {`${t("afkPlus.settings.adSample")}: ${name}: ${line}`}
              </p>
              <GameChat
                spoken={false}
                className="w-full"
                messages={[{ key: line, who: name, text: line, kind: "public" }]}
              />
              {count > 1 ? (
                <QuietLink tone="bone" onClick={() => setSample((shown) => (shown + 1) % count)}>
                  {t("afkPlus.settings.adNext")}
                </QuietLink>
              ) : null}
            </div>
          )}
        </>
      )}
    </div>
  );
}
