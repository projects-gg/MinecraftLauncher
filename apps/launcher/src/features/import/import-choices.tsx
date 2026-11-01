import { CheckCircle } from "@phosphor-icons/react";
import { cn, Select, Skeleton } from "@pl/ui";
import type { TFunction } from "i18next";
import { motion } from "motion/react";
import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { enter, stagger, useMotionPrefs } from "@/features/modcenter/motion";
import { Check } from "@/features/settings/cleanup/check";
import { formatBytes } from "@/lib/format";
import {
  LAUNCHER_NAMES,
  listCut,
  type MetaPart,
  type ProfileRow,
  profileMeta,
  type RowKey,
  SETTINGS_KEY,
  type SharedRow,
  type SourceRows,
} from "./model";
import type { ImportPicker } from "./use-import-picker";

/** A meta part in words (`import.<key>`); "vanilla" reads as Minecraft. */
function metaText(t: TFunction, part: MetaPart): string {
  const params = { ...part.params };
  if (part.key === "profile.version" && typeof params.loader === "string") {
    params.loader = params.loader === "vanilla" ? "Minecraft" : t(`loaders.${params.loader}`);
  }
  return t(`import.${part.key}`, params);
}

/**
 * The picked launchers' rows in one quiet panel: first the game settings (one row that stays in view
 * over the list: they come from one place, with the place on the right), then each launcher by its own name, what goes into the
 * Projects instance (a grid of small items), its worlds (each into the instance of the version it
 * was played on) and what becomes new instances (one row each, its size on the right). Rows rise
 * in once, a few milliseconds apart; sizes fill in where their skeletons held the place, so nothing
 * moves. Long lists scroll inside the panel, with a soft edge while more is below.
 */
export function ImportPanel({ picker, className }: { picker: ImportPicker; className?: string }) {
  const scroller = useRef<HTMLDivElement>(null);
  const [more, setMore] = useState(false);
  const rows = picker.rows;
  useEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const update = () =>
      setMore(element.scrollTop + element.clientHeight < element.scrollHeight - 2);
    // A list cut between two groups looks complete: it then ends half way into its last row
    // (`listCut`; at the smallest window the settings row above takes a row's height). The room
    // is the CSS limit, read again when the window changes.
    let room = 0;
    const readRoom = () => {
      element.style.maxHeight = "";
      room = Number.parseFloat(getComputedStyle(element).maxHeight) || 0;
    };
    const cut = () => {
      const rows = Array.from(
        element.querySelectorAll<HTMLElement>("[data-import-row]"),
        (row) => ({
          top: row.offsetTop,
          height: row.offsetHeight,
        }),
      );
      const at = listCut(rows, room, element.scrollHeight);
      const next = at === undefined ? "" : `${at}px`;
      if (element.style.maxHeight !== next) element.style.maxHeight = next;
    };
    const refresh = () => {
      cut();
      update();
    };
    const resize = () => {
      readRoom();
      refresh();
    };
    readRoom();
    refresh();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(refresh);
    observer?.observe(element);
    for (const child of Array.from(element.children)) observer?.observe(child);
    element.addEventListener("scroll", update, { passive: true });
    window.addEventListener("resize", resize);
    return () => {
      observer?.disconnect();
      element.removeEventListener("scroll", update);
      window.removeEventListener("resize", resize);
    };
  }, []);

  // Each row's place in the entrance (the stagger runs across the whole panel).
  let order = 0;
  return (
    <div
      className={cn(
        "relative overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-900/72 ring-1 ring-white/8",
        className,
      )}
    >
      {/* The game settings stay in view over the launchers' list; the list gives up their height. */}
      {picker.settings ? (
        <div className="border-b border-white/6 px-2 py-1">
          <SettingsLine picker={picker} order={order++} />
        </div>
      ) : null}
      <div
        ref={scroller}
        className={cn(
          "relative overflow-y-auto overscroll-contain px-2 py-2",
          picker.settings
            ? "max-h-[clamp(160px,calc(100dvh-456px),344px)]"
            : "max-h-[clamp(200px,calc(100dvh-400px),400px)]",
        )}
      >
        <div>
          {rows.map((source) => (
            <section
              key={source.source.id}
              aria-label={LAUNCHER_NAMES[source.source.kind]}
              className="border-t border-white/6 pt-1 first:border-t-0 first:pt-0 [&+section]:mt-2"
            >
              <p className="px-2 pt-1.5 pb-0.5 font-display text-[15px] font-semibold text-bone-50">
                {LAUNCHER_NAMES[source.source.kind]}
              </p>
              <SourceGroups source={source} picker={picker} nextOrder={() => order++} />
            </section>
          ))}
        </div>
      </div>
      <div
        aria-hidden
        className={cn(
          "pointer-events-none absolute inset-x-0 bottom-0 h-9 bg-[linear-gradient(to_top,rgb(18_16_22/0.96),transparent)] transition-opacity duration-200",
          more ? "opacity-100" : "opacity-0",
        )}
      />
    </div>
  );
}

function SourceGroups({
  source,
  picker,
  nextOrder,
}: {
  source: SourceRows;
  picker: ImportPicker;
  nextOrder: () => number;
}) {
  const { t } = useTranslation();
  // Worlds do not go into Projects: each goes into the instance of the version it was played on.
  const intoProjects = source.shared.filter((row) => row.kind !== "worlds");
  const worlds = source.shared.find((row) => row.kind === "worlds");
  return (
    <>
      {intoProjects.length > 0 ? (
        <Group heading={t("import.groups.projects")}>
          <div className="grid grid-cols-2 gap-x-0.5">
            {intoProjects.map((row) => (
              <SharedCell
                key={row.key}
                row={row}
                checked={picker.ticks.has(row.key)}
                onToggle={picker.toggle}
                order={nextOrder()}
              />
            ))}
          </div>
        </Group>
      ) : null}
      {worlds ? (
        <Group heading={t("import.groups.worlds")}>
          <WorldsLine
            row={worlds}
            checked={picker.ticks.has(worlds.key)}
            onToggle={picker.toggle}
            order={nextOrder()}
          />
        </Group>
      ) : null}
      {source.profiles.length > 0 ? (
        <Group heading={t("import.groups.instances")}>
          {source.profiles.map((row) => (
            <ProfileLine
              key={row.key}
              row={row}
              checked={picker.ticks.has(row.key)}
              onToggle={picker.toggle}
              order={nextOrder()}
            />
          ))}
        </Group>
      ) : null}
    </>
  );
}

function Group({ heading, children }: { heading: string; children: ReactNode }) {
  return (
    <div className="mt-1">
      <p className="px-2 pt-1.5 pb-1 text-[12px] font-medium text-ash-400">{heading}</p>
      {children}
    </div>
  );
}

/**
 * Rises 4 px into place, `order` beats after the first row (once, when the panel appears). It is a
 * row of the list (`data-import-row`: where the list may end, `listCut`).
 */
function Entrance({ order, children }: { order: number; children: ReactNode }) {
  const prefs = useMotionPrefs();
  return (
    <motion.div
      data-import-row
      initial={{ opacity: 0, transform: prefs.reduce ? "none" : "translateY(4px)" }}
      animate={{
        opacity: 1,
        transform: "translateY(0px)",
        transition: { ...enter(0.24), delay: stagger(order, 0.03, 10) },
      }}
    >
      {children}
    </motion.div>
  );
}

/** A size that fills in where its skeleton held the place. */
function Size({ bytes, on }: { bytes?: number; on: boolean }) {
  if (bytes === undefined) {
    return <Skeleton className="inline-block h-[11px] w-10 translate-y-[1px] align-baseline" />;
  }
  return (
    <span
      className={cn(
        "animate-[fade-in_180ms_var(--ease-out-expo)] transition-colors duration-150",
        on ? "text-bone-100" : "text-ash-400",
      )}
    >
      {formatBytes(bytes)}
    </span>
  );
}

const rowLabel =
  "flex items-center rounded-md px-2 py-[7px] transition-colors duration-150 has-[button:enabled]:cursor-pointer has-[button:enabled]:hover:bg-white/4";

function SharedCell({
  row,
  checked,
  onToggle,
  order,
}: {
  row: SharedRow;
  checked: boolean;
  onToggle: (key: RowKey, on: boolean) => void;
  order: number;
}) {
  const { t } = useTranslation();
  const id = useId();
  const meta = t(`import.rows.${row.kind}Meta`, { count: row.count });
  // The server list weighs nothing worth showing.
  const weighs = row.kind !== "servers";
  return (
    <Entrance order={order}>
      <label htmlFor={id} className={cn(rowLabel, "gap-2.5")}>
        <Check
          id={id}
          checked={checked}
          disabled={row.state !== "ready"}
          onCheckedChange={(on) => onToggle(row.key, on)}
        />
        <span className="min-w-0 flex-1">
          <span
            className={cn(
              "block truncate text-[13.5px] leading-tight",
              row.state === "ready" ? "text-bone-50" : "text-ash-300",
            )}
          >
            {t(`import.rows.${row.kind}`)}
          </span>
          <span className="mt-0.5 block truncate text-[12px] text-ash-400 tabular-nums">
            {meta}
            {weighs && row.state === "ready" ? (
              <>
                {", "}
                <Size bytes={row.bytes} on={checked} />
              </>
            ) : null}
          </span>
        </span>
      </label>
    </Entrance>
  );
}

/**
 * The game settings: one row above the launchers, since the Projects instance takes them from one
 * place. The place sits on the right: its name, or a select when there are several (picking one
 * means bringing them). Its second line says what comes, or that they came before.
 */
function SettingsLine({ picker, order }: { picker: ImportPicker; order: number }) {
  const { t } = useTranslation();
  const id = useId();
  const settings = picker.settings;
  if (!settings) return null;
  const { row, choice, on } = settings;
  return (
    <Entrance order={order}>
      <div className={cn(rowLabel, "gap-3 pr-1.5")}>
        <label htmlFor={id} className="flex min-w-0 flex-1 cursor-pointer items-center gap-3">
          <Check
            id={id}
            checked={on}
            onCheckedChange={(next) => picker.toggle(SETTINGS_KEY, next)}
          />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[13.5px] leading-tight text-bone-50">
              {t("import.rows.options")}
            </span>
            <span className="mt-0.5 block truncate text-[12px] text-ash-400">
              {choice.imported ? t("import.rows.optionsBefore") : t("import.rows.optionsMeta")}
            </span>
          </span>
        </label>
        {row.candidates.length > 1 ? (
          // "Nereden": the select picks where the settings come from, nothing else.
          <span className="flex min-w-0 shrink-0 items-center gap-2">
            <span aria-hidden className="text-[12.5px] text-ash-400">
              {t("import.rows.optionsFromLabel")}
            </span>
            <Select
              value={choice.key}
              onValueChange={picker.pickSettings}
              label={t("import.rows.optionsFrom")}
              options={row.candidates.map((c) => ({
                value: c.key,
                label: c.name,
                // An instance says whose it is; a launcher's own folder is named by the launcher.
                hint: c.profileId ? LAUNCHER_NAMES[c.kind] : undefined,
              }))}
              className="w-[176px]"
            />
          </span>
        ) : (
          <span className="max-w-[46%] shrink-0 truncate text-right text-[12.5px] text-ash-400">
            {choice.profileId
              ? t("import.rows.optionsFromInstance", { name: choice.name })
              : t(`import.from.${choice.kind}`)}
          </span>
        )}
      </div>
    </Entrance>
  );
}

/** The worlds of a launcher: one row like an instance's, its size on the right. */
function WorldsLine({
  row,
  checked,
  onToggle,
  order,
}: {
  row: SharedRow;
  checked: boolean;
  onToggle: (key: RowKey, on: boolean) => void;
  order: number;
}) {
  const { t } = useTranslation();
  const id = useId();
  return (
    <Entrance order={order}>
      <label htmlFor={id} className={cn(rowLabel, "gap-3")}>
        <Check
          id={id}
          checked={checked}
          disabled={row.state !== "ready"}
          onCheckedChange={(on) => onToggle(row.key, on)}
        />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[13.5px] leading-tight text-bone-50">
            {t("import.rows.worlds")}
          </span>
          <span className="mt-0.5 block truncate text-[12px] text-ash-400 tabular-nums">
            {t("import.rows.worldsMeta", { count: row.count })}
          </span>
        </span>
        <span className="w-[68px] shrink-0 text-right text-[12.5px] tabular-nums">
          <Size bytes={row.bytes} on={checked} />
        </span>
      </label>
    </Entrance>
  );
}

function ProfileLine({
  row,
  checked,
  onToggle,
  order,
}: {
  row: ProfileRow;
  checked: boolean;
  onToggle: (key: RowKey, on: boolean) => void;
  order: number;
}) {
  const { t } = useTranslation();
  const id = useId();
  const meta = profileMeta(row)
    .map((part) => metaText(t, part))
    .join(", ");
  const ready = row.state === "ready";
  const body = (
    <>
      <span className="min-w-0 flex-1">
        <span
          className={cn(
            "block truncate text-[13.5px] leading-tight",
            ready ? "text-bone-50" : "text-ash-300",
          )}
        >
          {row.profile.name}
        </span>
        {meta ? (
          <span className="mt-0.5 block truncate text-[12px] text-ash-400 tabular-nums">
            {meta}
          </span>
        ) : null}
      </span>
      <span className="w-[68px] shrink-0 text-right text-[12.5px] tabular-nums">
        {ready && (row.bytes === undefined || row.bytes > 0) ? (
          <Size bytes={row.bytes} on={checked} />
        ) : null}
      </span>
    </>
  );
  return (
    <Entrance order={order}>
      {row.state === "imported" ? (
        <div className={cn(rowLabel, "gap-3")}>
          <CheckCircle
            size={18}
            weight="fill"
            className="shrink-0 text-jade-400"
            aria-label={t("import.profile.imported")}
          />
          {body}
        </div>
      ) : (
        <label htmlFor={id} className={cn(rowLabel, "gap-3")}>
          <Check
            id={id}
            checked={checked}
            disabled={!ready}
            onCheckedChange={(on) => onToggle(row.key, on)}
          />
          {body}
        </label>
      )}
    </Entrance>
  );
}

/**
 * The line under the panel: the selected size and the disk's free space, or why it waits. It
 * starts on the panel's own edge, as the words above and the buttons below do.
 */
export function ImportTotals({ picker, className }: { picker: ImportPicker; className?: string }) {
  const { t } = useTranslation();
  const short = !picker.enough;
  // Until every ticked row is measured the line holds its place.
  if (picker.pending && !short) {
    return (
      <div className={cn("flex min-h-[18px] items-center", className)} aria-busy>
        <Skeleton className="h-[11px] w-52" />
      </div>
    );
  }
  let text: string;
  if (picker.empty) text = t("import.space.nothing");
  else if (short) text = t("import.space.short");
  else if (picker.freeBytes !== undefined) {
    text = t("import.space.total", {
      size: formatBytes(picker.total),
      free: formatBytes(picker.freeBytes),
    });
  } else text = t("import.space.totalNoFree", { size: formatBytes(picker.total) });
  return (
    <p
      aria-live="polite"
      className={cn(
        "min-h-[18px] text-[12.5px] tabular-nums transition-colors duration-200",
        "animate-[fade-in_180ms_var(--ease-out-expo)]",
        short ? "text-flare-400" : "text-ash-400",
        className,
      )}
    >
      {text}
    </p>
  );
}
