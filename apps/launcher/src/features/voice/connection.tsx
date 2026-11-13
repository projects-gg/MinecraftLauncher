// SQOL-U3: a connection that is not good: its bars, and their words ("Bağlantı: Zayıf") in a
// tooltip and to screen readers. A stop of its own on the keyboard, so the words are there without
// a pointer; never inside a button (a stop inside a button is lost to it).
import { cn, Tooltip } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { VoiceQuality } from "@/lib/api/voice-types";
import { QualityBars } from "./faces";
import { connectionWords } from "./panel-rules";

export function ConnectionMark({
  quality,
  className,
}: {
  quality: VoiceQuality;
  className?: string;
}) {
  const { t } = useTranslation();
  if (quality === "good") return null;
  const words = connectionWords(quality, t);
  return (
    <Tooltip content={words}>
      <span
        role="img"
        // biome-ignore lint/a11y/noNoninteractiveTabindex: the bars' words reach the keyboard too
        tabIndex={0}
        aria-label={words}
        className={cn(
          "flex shrink-0 rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
          className,
        )}
      >
        <QualityBars quality={quality} />
      </span>
    </Tooltip>
  );
}
