// Profile marks (SOC-VERIFY): the mark Projects' staff gave someone in Studio, right of their name,
// in the interface's language (@pl/ui's VerifiedMark draws it, the same everywhere). Where it goes
// (docs/design.md §6, "Profile marks"): wherever a name labels its person (rows, cards, headers, a
// message's sender) and in a line that starts from them (a toast, a Home line: `MarkedSentence`);
// the player's own too where they show as others see them. Never beside a name a sentence only
// mentions ("Ayşe aracılığıyla"), never in the system's notifications (text only), never on the
// title bar's Minecraft account (another identity).
import { cn, type MarkSize, VerifiedMark } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { ProfileMark } from "@/lib/api/social-types";
import { markLabel, sentenceParts } from "./marks";

export { markedName, markLabel } from "./marks";

/**
 * A line about someone that starts from them (a toast's title, a Home line: "Ayşe seni birlikte
 * oynamaya çağırıyor") with their mark right after their name. The sentence is translated with a
 * slot for the name and split there, so every language keeps its word order; without a mark (or a
 * sentence without the name) it is the plain sentence. The name and its mark never break apart. No
 * tooltip inside a sentence: the mark's name is read with it.
 */
export function MarkedSentence({
  text,
  name,
  mark,
  size = 14,
}: {
  /** The sentence for a name (`(name) => t("…", { name })`). */
  text: (name: string) => string;
  name: string;
  mark: ProfileMark | undefined;
  size?: MarkSize;
}) {
  const parts = mark ? sentenceParts(text) : null;
  if (!mark || !parts) return <>{text(name)}</>;
  return (
    <>
      {parts[0]}
      <span className="whitespace-nowrap">
        {name}
        <PersonMark
          mark={mark}
          size={size}
          tooltip={false}
          className="ml-1 inline-block align-[-0.2em]"
        />
      </span>
      {parts[1]}
    </>
  );
}

/**
 * A person's name where it labels a row (a request, a lookup, a blocked player): the name truncates,
 * their mark right after it stays whole. `className` styles the name's words.
 */
export function NameWithMark({
  name,
  mark,
  size = 14,
  className,
}: {
  name: string;
  mark: ProfileMark | undefined;
  size?: MarkSize;
  className?: string;
}) {
  if (!mark) return <span className={cn("block truncate", className)}>{name}</span>;
  return (
    <span className="flex min-w-0 items-center gap-1">
      <span className={cn("min-w-0 truncate", className)}>{name}</span>
      <PersonMark mark={mark} size={size} />
    </span>
  );
}

/** Someone's mark, or nothing. `tooltip={false}` inside a menu row or another tooltip. */
export function PersonMark({
  mark,
  size = 14,
  tooltip = true,
  className,
}: {
  mark: ProfileMark | undefined;
  size?: MarkSize;
  tooltip?: boolean;
  className?: string;
}) {
  const { t } = useTranslation();
  if (!mark) return null;
  return (
    <VerifiedMark
      kind={mark}
      size={size}
      label={markLabel(mark, t)}
      tooltip={tooltip}
      className={className}
    />
  );
}
