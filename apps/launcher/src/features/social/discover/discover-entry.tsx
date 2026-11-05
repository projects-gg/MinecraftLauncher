// "Ekip bul" (CREW-DISCOVER) on "Ekip kur ya da katıl": one quiet row that opens the page, above
// founding and joining by code. A player without a crew sees it first thing on the crew page.
import { CaretRight, Compass } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import { useSocialUi } from "../store";

export function DiscoverEntry() {
  const { t } = useTranslation();
  const goTo = useSocialUi((s) => s.goTo);
  return (
    <button
      type="button"
      onClick={() => goTo("discover")}
      className="group mb-4 flex w-full items-center gap-3 rounded-[var(--radius-panel)] bg-obsidian-950/40 px-4 py-3 text-left ring-1 ring-white/8 outline-none transition-[background-color,box-shadow,scale] duration-150 hover:bg-white/[0.04] hover:ring-white/14 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.99]"
    >
      <span
        aria-hidden
        className="chamfer flex size-9 shrink-0 items-center justify-center bg-white/7 text-bone-100 [--cut:6px]"
      >
        <Compass size={19} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-[14px] font-semibold text-bone-50">
          {t("discover.entry.title")}
        </span>
        <span className="mt-0.5 block text-[12.5px] leading-snug text-ash-300">
          {t("discover.entry.line")}
        </span>
      </span>
      <CaretRight
        size={15}
        weight="bold"
        aria-hidden
        className="shrink-0 text-ash-400 transition-transform duration-150 group-hover:translate-x-0.5"
      />
    </button>
  );
}
