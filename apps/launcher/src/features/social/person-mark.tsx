// Profile badges (BADGES-UI; SOC-VERIFY's marks grown into a catalogue): what Projects gave someone,
// right of their name, in the interface's language (@pl/ui's ProfileMarks and ProfileMarkList draw
// them, the same everywhere). Where (docs/design.md §6, "Profile badges"): beside a name wherever it
// labels its person (rows, headers, a message's sender, member lists, requests and invites) and in
// a line that starts from them (a toast, a Home line: `MarkedSentence`), only the first badge, its
// tooltip and a screen reader naming them all; under the name on a person's card every badge with
// its name (`PersonMarkList`). The player's own where they show as others see them. Never beside a
// name a sentence only mentions ("Ayşe aracılığıyla"), never in the system's notifications (text
// only), never on the title bar's Minecraft account (another identity).
import { cn, type MarkItem, type MarkSize, ProfileMarkList, ProfileMarks } from "@pl/ui";
import { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { BUILTIN_MARK_DEFS, markedName, markItems, sentenceParts } from "./marks";
import { useSocialUi } from "./store";

export { markedName } from "./marks";

/** Someone's badges this launcher draws (the catalogue knows them), named in the interface's
 * language, the most important first. */
export function useMarks(keys: readonly string[] | undefined): MarkItem[] {
  const defs = useSocialUi((s) => s.view.markDefs) ?? BUILTIN_MARK_DEFS;
  const { i18n } = useTranslation();
  const english = i18n.language === "en";
  const id = keys?.join(",") ?? "";
  // biome-ignore lint/correctness/useExhaustiveDependencies: the keys' contents, not their array
  return useMemo(() => markItems(keys, defs, english), [id, defs, english]);
}

/** How a row names someone for screen readers: "Ayşe, Projects yöneticisi" (the name alone
 * without badges). */
export function useMarkedName(name: string, keys: readonly string[] | undefined): string {
  const { t } = useTranslation();
  return markedName(name, useMarks(keys), t);
}

/**
 * `useMarkedName` for a component that names someone after its early returns (a hook's place
 * stays fixed): the namer, called with the name and the badge keys.
 */
export function useMarkNamer(): (name: string, keys: readonly string[] | undefined) => string {
  const defs = useSocialUi((s) => s.view.markDefs) ?? BUILTIN_MARK_DEFS;
  const { t, i18n } = useTranslation();
  const english = i18n.language === "en";
  return useCallback(
    (name, keys) => markedName(name, markItems(keys, defs, english), t),
    [defs, english, t],
  );
}

/**
 * A line about someone that starts from them (a toast's title, a Home line: "Ayşe seni birlikte
 * oynamaya çağırıyor") with their first badge right after their name. The sentence is translated
 * with a slot for the name and split there, so every language keeps its word order; without a
 * badge (or a sentence without the name) it is the plain sentence. The name and its badge never
 * break apart. No tooltip inside a sentence: the badges' names are read with it.
 */
export function MarkedSentence({
  text,
  name,
  marks,
  size = 14,
}: {
  /** The sentence for a name (`(name) => t("…", { name })`). */
  text: (name: string) => string;
  name: string;
  marks: readonly string[] | undefined;
  size?: MarkSize;
}) {
  const items = useMarks(marks);
  const parts = items.length > 0 ? sentenceParts(text) : null;
  if (!parts) return <>{text(name)}</>;
  return (
    <>
      {parts[0]}
      <span className="whitespace-nowrap">
        {name}
        <ProfileMarks
          marks={items}
          size={size}
          tooltip={false}
          className="ml-1 inline-flex align-[-0.2em]"
        />
      </span>
      {parts[1]}
    </>
  );
}

/**
 * A person's name where it labels a row (a request, a lookup, a blocked player): the name
 * truncates, their first badge right after it stays whole. `className` styles the name's words.
 */
export function NameWithMark({
  name,
  marks,
  size = 14,
  className,
}: {
  name: string;
  marks: readonly string[] | undefined;
  size?: MarkSize;
  className?: string;
}) {
  const items = useMarks(marks);
  if (items.length === 0) return <span className={cn("block truncate", className)}>{name}</span>;
  return (
    <span className="flex min-w-0 items-center gap-1">
      <span className={cn("min-w-0 truncate", className)}>{name}</span>
      <ProfileMarks marks={items} size={size} />
    </span>
  );
}

/** Someone's first badge (the tooltip names them all), or nothing. `tooltip={false}` inside a
 * menu row or another tooltip. */
export function PersonMark({
  marks,
  size = 14,
  tooltip = true,
  className,
}: {
  marks: readonly string[] | undefined;
  size?: MarkSize;
  tooltip?: boolean;
  className?: string;
}) {
  const items = useMarks(marks);
  if (items.length === 0) return null;
  return <ProfileMarks marks={items} size={size} tooltip={tooltip} className={className} />;
}

/**
 * Every badge of someone with its name, under the name block on a person's card (a friend's, a
 * crewmate's): one per line, as the badge's tooltip lists them (a wrapped row read as a cloud of
 * chips once a third badge broke onto its own line). Nothing without badges.
 */
export function PersonMarkList({
  marks,
  className,
}: {
  marks: readonly string[] | undefined;
  className?: string;
}) {
  const items = useMarks(marks);
  if (items.length === 0) return null;
  return <ProfileMarkList marks={items} tone="inherit" className={cn("text-ash-300", className)} />;
}
