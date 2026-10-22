// A cloud account's state in one line, as on the desktop cards: the server bars and the words. Only
// the uptime and the reconnect countdown tick, and only while somebody can see them.
import { cn, SignalBars, usePerf } from "@pl/ui";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { CloudAccount } from "./api";
import { cloudStatus } from "./status";

export function CloudStateLine({
  account,
  skewMs,
  className,
  showHint = false,
}: {
  account: CloudAccount;
  /** The service's clock minus this computer's. */
  skewMs: number;
  className?: string;
  showHint?: boolean;
}) {
  const { t, i18n } = useTranslation();
  const { unseen } = usePerf();
  const [ticks, tick] = useState(0);
  const status = cloudStatus(account, Date.now() + skewMs, t, i18n.language === "en" ? "en" : "tr");
  const wait = unseen ? null : status.nextChangeMs;
  useEffect(() => {
    void ticks;
    if (wait === null) return;
    const id = window.setTimeout(() => tick((n) => n + 1), wait);
    return () => window.clearTimeout(id);
  }, [wait, ticks]);
  return (
    <div className={cn("min-w-0", className)}>
      <p
        className="flex min-w-0 items-center gap-2 text-[12.5px]"
        // A long stop reason is cut in the line; the whole of it stays one hover away.
        title={status.tone === "error" ? status.text : undefined}
      >
        <SignalBars state={status.signal} />
        <span
          className={cn(
            "truncate",
            status.tone === "ok" && "text-ash-300",
            status.tone === "muted" && "text-ash-300",
            status.tone === "warn" && "text-gold-300",
            status.tone === "error" && "text-flare-400",
          )}
        >
          {status.text}
        </span>
      </p>
      {showHint && status.hint ? (
        <p className="mt-1 pl-[23px] text-[12px] leading-snug text-ash-300">{status.hint}</p>
      ) : null}
    </div>
  );
}
