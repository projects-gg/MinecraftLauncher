// A private line's direction (TASTE-SWEEP): the server's own words never say plainly whose a reply
// is, so the console marks it with its own icons: an envelope for one to you, the send button's
// plane and "Sen" for your own. Outside the copied words (select-none) and the line's own text, and
// outside a line's text-indent (the site's hanging indent would pull "Sen" over the clock).
import { EnvelopeSimple, PaperPlaneRight } from "@phosphor-icons/react";
import type { InventoryLang } from "../inventory/texts";
import { CONSOLE_TEXTS } from "./texts";

export function ConsoleDirection({ dir, lang }: { dir: unknown; lang: InventoryLang }) {
  const t = CONSOLE_TEXTS[lang];
  if (dir === "in") {
    return (
      <span
        role="img"
        aria-label={t.pmIn}
        className="mr-1.5 inline-flex indent-0 align-[-2px] text-gold-300 select-none"
      >
        <EnvelopeSimple size={13} weight="bold" aria-hidden />
      </span>
    );
  }
  if (dir === "out") {
    return (
      <span
        role="img"
        aria-label={t.pmOutSaid}
        className="mr-1.5 inline-flex items-center gap-1 indent-0 align-[-1px] font-sans text-[11.5px] font-medium text-ash-300 select-none"
      >
        <PaperPlaneRight size={12} weight="bold" aria-hidden />
        <span aria-hidden>{t.pmOut}</span>
      </span>
    );
  }
  return null;
}
