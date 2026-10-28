// "Para birimi" (TRADE-ANY, 28 Oct 2026): the name of the server's money, written after amounts.
// The player names it where the server's profile does not; a profile's own name (Projects Survival
// and SkyBlock: ripple) shows as words with a lock, never as a field (the site's way, SITE-W2). One
// line under the player's field: the hint, or that the name cannot be used (the panel's check, or
// the service's word on a save).
import { Lock } from "@phosphor-icons/react";
import { cn } from "@pl/ui";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { inputClass } from "../../settings/controls";
import { CURRENCY_MAX, type CurrencyFieldState } from "./model";

export function CurrencyField({
  state,
  value,
  refused,
  onChange,
}: {
  state: CurrencyFieldState;
  /** The player's own name as typed. */
  value: string;
  refused: boolean;
  onChange: (text: string) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  if (state.kind === "hidden") return null;
  if (state.kind === "fixed") {
    return (
      <dl className="m-0 flex flex-col gap-1.5 py-2.5">
        <dt className="text-[13px] font-medium text-bone-100">{t("afkPlus.settings.currency")}</dt>
        <dd className="m-0 flex items-center gap-1.5 text-[13.5px] text-bone-50">
          <Lock size={14} aria-hidden className="shrink-0 text-ash-400" />
          {state.label}
        </dd>
        <dd className="m-0 text-[12px] leading-[17px] text-ash-400">
          {t("afkPlus.settings.currencyFixed")}
        </dd>
      </dl>
    );
  }
  return (
    <div className="flex flex-col gap-1.5 py-2.5">
      <label htmlFor={id} className="text-[13px] font-medium text-bone-100">
        {t("afkPlus.settings.currency")}
      </label>
      <input
        id={id}
        value={value}
        maxLength={CURRENCY_MAX}
        autoComplete="off"
        spellCheck={false}
        aria-invalid={refused ? true : undefined}
        aria-describedby={`${id}-line`}
        onChange={(event) => onChange(event.target.value)}
        className={cn(
          inputClass,
          "h-[34px] max-w-[220px] px-3",
          refused && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
        )}
      />
      <p
        id={`${id}-line`}
        role={refused ? "alert" : undefined}
        className={cn("text-[12px] leading-[17px]", refused ? "text-flare-300" : "text-ash-400")}
      >
        {refused ? t("afkPlus.settings.currencyRefused") : t("afkPlus.settings.currencyHelp")}
      </p>
    </div>
  );
}
