// A clicked command or link waiting for the player's word (CONSOLE-COLOR): the exact command or
// the whole address, and whose line it came from. The host shows it in the command line's place
// (one gold button on screen) and answers its buttons: a command goes the typed command's way, a
// link opens as any outside link does. Nothing happens until "Çalıştır" or "Aç".
import { type KeyboardEvent, useEffect, useId, useRef } from "react";
import { cn } from "../../lib/cn";
import { Button } from "../button";
import { fill, type InventoryLang } from "../inventory/texts";
import { type ConsoleClick, domainParts, urlParts } from "./looks";
import { CONSOLE_TEXTS } from "./texts";

export interface ConsoleActionCardProps {
  /** A `run_command` or an `open_url` click (checked: `safeClick`). */
  click: ConsoleClick;
  lang: InventoryLang;
  /** The player whose line it is (a chat or private line); none: the server's. */
  who?: string | null;
  /** The line's time as the console shows it. */
  clock: string;
  /** Where it came from when it is no line ("Sunucu penceresinden"); else the line's sender. */
  source?: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
  className?: string;
}

export function ConsoleActionCard({
  click,
  lang,
  who,
  clock,
  source: from,
  busy,
  onConfirm,
  onCancel,
  className,
}: ConsoleActionCardProps) {
  const t = CONSOLE_TEXTS[lang];
  const titleId = useId();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const run = click.a === "run_command";
  const link = run ? null : urlParts(click.v);
  // The safe choice has the focus: Enter never runs a command the player has not read.
  useEffect(() => {
    cancelRef.current?.focus();
  }, []);
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
    }
  };
  const source = from ?? (who ? fill(t.fromPlayer, { name: who }) : t.fromServer);
  return (
    // A group of its own: Escape cancels from anywhere in it.
    <fieldset
      aria-labelledby={titleId}
      onKeyDown={onKeyDown}
      className={cn(
        "min-w-0 rounded-[10px] border border-white/[0.08] bg-obsidian-800/80 p-4 font-sans",
        className,
      )}
    >
      <p id={titleId} className="text-[14px] font-medium text-bone-50">
        {run ? t.runTitle : t.openTitle}
      </p>
      {run || !link ? (
        <p className="mt-2 rounded-md bg-obsidian-900 px-3 py-2 font-mono text-[13px] break-all text-bone-100 select-text">
          {click.v}
        </p>
      ) : (
        // The registrable domain alone and first, unbroken and in bold: it is what a player reads to
        // know where a link goes (a host that starts with another site's name shows the real one).
        // The whole address follows, muted, nothing of it hidden.
        <div className="mt-2 rounded-md bg-obsidian-900 px-3 py-2 font-mono select-text">
          <p className="overflow-x-auto text-[14px] font-semibold whitespace-nowrap text-bone-50">
            {domainParts(link.host).domain}
          </p>
          <p className="mt-0.5 text-[12.5px] break-all text-ash-400">
            {link.host}
            {link.rest}
          </p>
        </div>
      )}
      <p className="mt-2 text-[12.5px] text-ash-400">
        {source}, {clock}
      </p>
      <div className="mt-3 flex gap-2">
        <Button variant="primary" size="md" disabled={busy} onClick={onConfirm}>
          {run ? t.run : t.open}
        </Button>
        <Button ref={cancelRef} variant="secondary" size="md" onClick={onCancel}>
          {t.cancel}
        </Button>
      </div>
    </fieldset>
  );
}
