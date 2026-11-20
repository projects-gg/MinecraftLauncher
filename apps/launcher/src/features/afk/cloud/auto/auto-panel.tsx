// The account panel's "Balık" and "Kazma" tabs (FISHMINE, services/cloud-afk README "Auto on the live
// socket"): today's numbers as one quiet line in the game's own form, one reserved line that says
// what the mode does (with "Duraklat" or "Devam Et" at its end), today's items as the game's slots,
// the last catches, the rod or pickaxe with the game's wear bar, the block it looks at, where it
// looks, and the disclaimer. Items are the game's own textures, named from the game's item data;
// the words are the site's. It streams only while the tab is on screen (the console owns the one
// `auto` watch for both tabs).
import { Info, Pause, Play } from "@phosphor-icons/react";
import { Button, cn, EmptyState, enter, leave } from "@pl/ui";
import { ItemSprite } from "@pl/ui/items";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { uiLocale } from "@/lib/format";
import { openWebLink } from "@/lib/links";
import { TEXT_LINK } from "@/lib/text-link";
import type { CloudAccount } from "../api";
import { WalkButton } from "../movement/movement-panel";
import { ItemTile, PlusItemArt } from "../plus/item-icon";
import { itemLabel, usePlusItemData } from "../plus/items";
import { serverLabel } from "../status";
import {
  AIM_DIRECTIONS,
  AIM_TILTS,
  type AimDirection,
  type AimTilt,
  type AutoMode,
  type AutoOp,
  canPause,
  type FishState,
  granted,
  type MineState,
  needsResume,
  offKey,
  rateText,
  refusalKey,
  stateLine,
  wear,
} from "./model";
import type { CloudAuto } from "./use-cloud-auto";

/** The limited servers page of the site, in the launcher's language (both lists, their reasons). */
export function limitedPage(lang: "tr" | "en"): string {
  return lang === "en"
    ? "https://afk.projects.gg/en/limited-servers"
    : "https://afk.projects.gg/sinirli-sunucular";
}

/** A refusal shows in the state line for this long. */
const REFUSAL_MS = 4000;

/** The mode's own item: the empty states' picture and the tool row's stand-in. */
const MODE_ITEM: Record<AutoMode, string> = {
  fishing: "minecraft:fishing_rod",
  mining: "minecraft:iron_pickaxe",
};

type Aim = { dir: AimDirection | null; tilt: AimTilt };
const NO_AIM: Aim = { dir: null, tilt: "level" };
/** Where this launcher last aimed each account (the tabs share it; no frame says where it looks). */
const lastAim = new Map<string, Aim>();

/** The line under every mode, on every surface (owner, 20 Nov 2026). */
export function Disclaimer({ className }: { className?: string }) {
  const { t } = useTranslation();
  return (
    <p className={cn("flex items-start gap-2 text-[12px] leading-snug text-ash-400", className)}>
      <Info size={14} aria-hidden className="mt-px shrink-0" />
      <span>{t("cloudAfkAuto.disclaimer")}</span>
    </p>
  );
}

function Quiet({ mode, text, action }: { mode: AutoMode; text: string; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-2 pb-4">
      <EmptyState
        icon={<ItemSprite id={MODE_ITEM[mode]} size={32} />}
        title={text}
        action={action}
        className="py-12"
      />
      <Disclaimer className="max-w-[46ch]" />
    </div>
  );
}

/** A number in the game's own form with its word after it. */
function Figure({
  value,
  label,
  tone,
  big,
}: {
  value: number;
  label: string;
  tone?: "gold";
  big?: boolean;
}) {
  return (
    <span className="flex items-baseline gap-1.5">
      <span
        className={cn(
          "font-display font-semibold tabular-nums",
          big ? "text-[22px] leading-none" : "text-[15px] leading-none",
          tone === "gold" && value > 0 ? "text-gold-200" : "text-bone-50",
        )}
      >
        {new Intl.NumberFormat(uiLocale()).format(value)}
      </span>
      <span className="text-[12.5px] text-ash-400">{label}</span>
    </span>
  );
}

/** One kind of today's items: the game's slot with its count, the name on hover and for readers. */
function ItemCount({ id, n, name }: { id: string; n: number; name: string }) {
  return (
    <li title={`${name} \u00d7 ${n}`}>
      <span role="img" aria-label={`${name}, ${n}`} className="relative block">
        <ItemTile id={id} size={40} />
        <span
          aria-hidden
          className="pointer-events-none absolute right-[3px] bottom-[1px] font-display text-[12px] leading-none font-semibold text-bone-50 tabular-nums [text-shadow:1px_1px_0_rgb(0_0_0/0.75)]"
        >
          {n > 1 ? n : ""}
        </span>
      </span>
    </li>
  );
}

/** The rod or the pickaxe in hand, with the game's wear bar while it is worn. */
function ToolTile({ item, dur, max }: { item: string; dur: number; max: number }) {
  const worn = max > 0 && dur < max;
  const { fill, color } = wear(dur, max);
  return (
    <span className="relative block shrink-0">
      <ItemTile id={item} size={40} />
      {worn ? (
        <span aria-hidden className="absolute top-[30px] left-[8px] h-[4px] w-[26px] bg-black">
          <span
            className="block h-[2px]"
            style={{ width: `${Math.max(1, Math.round(26 * fill))}px`, backgroundColor: color }}
          />
        </span>
      ) : null}
    </span>
  );
}

function Heading({ children }: { children: ReactNode }) {
  return <span className="text-[13px] font-medium text-bone-100">{children}</span>;
}

export function AutoPanel({
  mode,
  account,
  auto,
  onReconnect,
  onUnavailable,
}: {
  mode: AutoMode;
  account: CloudAccount;
  /** The console's one stream for both tabs. */
  auto: CloudAuto;
  onReconnect: () => void;
  /** Studio switched the mode off: the tab goes. */
  onUnavailable: () => void;
}) {
  const version = account.version && account.version !== "auto" ? account.version : null;
  return (
    <PlusItemArt minecraft={version} server={serverLabel(account.server)}>
      <AutoBody
        mode={mode}
        account={account}
        auto={auto}
        version={version}
        onReconnect={onReconnect}
        onUnavailable={onUnavailable}
      />
    </PlusItemArt>
  );
}

function AutoBody({
  mode,
  account,
  auto,
  version,
  onReconnect,
  onUnavailable,
}: {
  mode: AutoMode;
  account: CloudAccount;
  auto: CloudAuto;
  version: string | null;
  onReconnect: () => void;
  onUnavailable: () => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language === "en" ? "en" : "tr";
  const locale = uiLocale();
  const { data } = usePlusItemData(version);
  const name = useCallback(
    (id: string) => itemLabel(id, { data, custom: [], unnamed: "" }, lang),
    [data, lang],
  );
  const state: FishState | MineState | null =
    mode === "fishing" ? auto.state.fishing : auto.state.mining;
  const inGame = account.live.state === "running";
  const [refusal, setRefusal] = useState<string | null>(null);
  const [aim, setAim] = useState<Aim>(() => lastAim.get(account.id) ?? NO_AIM);
  const refusalTimer = useRef<number | null>(null);

  useEffect(() => {
    if (state?.state === "off" && state.reason === "service") onUnavailable();
  }, [state?.state, state?.reason, onUnavailable]);
  useEffect(() => {
    setAim(lastAim.get(account.id) ?? NO_AIM);
  }, [account.id]);
  useEffect(
    () => () => {
      if (refusalTimer.current !== null) window.clearTimeout(refusalTimer.current);
    },
    [],
  );

  const act = useCallback(
    async (op: AutoOp) => {
      const result = await auto.send(op);
      if (refusalTimer.current !== null) window.clearTimeout(refusalTimer.current);
      if (result.ok) {
        setRefusal(null);
        return;
      }
      const key = refusalKey(mode, result.error, op.action === "aim");
      if (key) {
        setRefusal(t(`cloudAfkAuto.${key}`));
        refusalTimer.current = window.setTimeout(() => setRefusal(null), REFUSAL_MS);
      }
    },
    [auto, mode, t],
  );

  // A direction sends both angles; a look waits for a direction (no frame says where it looks now).
  const aimAt = (next: Aim) => {
    setAim(next);
    lastAim.set(account.id, next);
    if (next.dir === null) return;
    void act({ action: "aim", yaw: AIM_DIRECTIONS[next.dir], pitch: AIM_TILTS[next.tilt] });
  };

  // What holds it, from the account first (no need to wait for the machine), then the machine's word.
  const reason =
    state?.state === "off" ? (state.reason ?? "service") : !inGame && !state ? "not_running" : null;
  if (reason === "restart") {
    return (
      <Quiet
        mode={mode}
        text={t(`cloudAfkAuto.${mode}.restart`)}
        action={
          <Button size="sm" variant="secondary" onClick={onReconnect}>
            {t("cloudAfkAuto.reconnect")}
          </Button>
        }
      />
    );
  }
  if (reason === "limited") {
    const limited = account.features?.[mode]?.limited;
    return (
      <Quiet
        mode={mode}
        text={limited?.[lang] || t(`cloudAfkAuto.${mode}.limited`)}
        action={
          <button
            type="button"
            className={cn(TEXT_LINK, "text-[13px] font-medium")}
            onClick={() => openWebLink(limitedPage(lang))}
          >
            {t("cloudAfkAuto.limitedLink")}
          </button>
        }
      />
    );
  }
  if (reason) return <Quiet mode={mode} text={t(`cloudAfkAuto.${mode}.${offKey(reason)}`)} />;

  const fishing = mode === "fishing" ? (state as FishState | null) : null;
  const mining = mode === "mining" ? (state as MineState | null) : null;
  const target = mining?.target;
  const targetId = target?.id || null;
  const word = stateLine(mode, state);
  const line = refusal
    ? { text: refusal, tone: "text-flare-400" }
    : word
      ? {
          text: t(`cloudAfkAuto.${word.key}`, word.values),
          tone: word.tone === "active" ? "text-gold-300" : "text-ash-300",
        }
      : null;
  const resume = needsResume(state);
  const pausable = canPause(state);
  const aimable = granted(state);
  const items = (fishing?.today.items ?? mining?.today.items ?? []).filter((item) => item.n > 0);
  const tool = fishing ? fishing.rod : (mining?.tool ?? null);
  const rate = fishing?.today.perHour ?? mining?.today.perMinute ?? null;
  const time = new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" });
  const lastCatches = fishing?.today.last ?? [];

  return (
    <div data-auto-panel={mode} className="@container/auto flex flex-col gap-4 pb-4">
      <section
        aria-label={t(`cloudAfkAuto.${mode}.items`)}
        className="flex min-h-7 flex-wrap items-baseline justify-between gap-x-6 gap-y-1"
      >
        <div className="flex flex-wrap items-baseline gap-x-5 gap-y-1">
          {mode === "fishing" ? (
            <>
              <Figure
                big
                value={fishing?.today.catches ?? 0}
                label={t("cloudAfkAuto.fishing.caught")}
              />
              <Figure value={fishing?.today.fish ?? 0} label={t("cloudAfkAuto.fishing.fish")} />
              <Figure
                value={fishing?.today.treasure ?? 0}
                label={t("cloudAfkAuto.fishing.treasure")}
                tone="gold"
              />
              <Figure value={fishing?.today.junk ?? 0} label={t("cloudAfkAuto.fishing.junk")} />
            </>
          ) : (
            <Figure big value={mining?.today.blocks ?? 0} label={t("cloudAfkAuto.mining.broken")} />
          )}
        </div>
        {rate !== null && rate > 0 ? (
          <span className="shrink-0 text-[12.5px] text-ash-300 tabular-nums">
            {t(`cloudAfkAuto.${mode}.rate`, { n: rateText(rate, locale) })}
          </span>
        ) : null}
      </section>

      {/* One reserved line, nothing below moves: a refusal (4 s), else what the mode does; its one
          action at the end ("Devam Et" in gold where only the player can go on). */}
      <div className="flex min-h-8 items-center gap-3">
        <div aria-live="polite" className="grid min-w-0 flex-1">
          <AnimatePresence initial={false}>
            {line ? (
              <motion.p
                key={line.text}
                className={cn("[grid-area:1/1] text-[13.5px] leading-5 text-pretty", line.tone)}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: enter(0.15) }}
                exit={{ opacity: 0, transition: leave(0.1) }}
              >
                {line.text}
              </motion.p>
            ) : null}
          </AnimatePresence>
        </div>
        {resume ? (
          <Button
            size="sm"
            variant="primary"
            icon={<Play size={14} weight="fill" />}
            onClick={() => void act({ mode, action: "resume" })}
          >
            {t("cloudAfkAuto.resume")}
          </Button>
        ) : (
          <Button
            size="sm"
            variant="secondary"
            icon={<Pause size={14} weight="fill" />}
            onClick={() => void act({ mode, action: "pause" })}
            tabIndex={pausable ? undefined : -1}
            aria-hidden={pausable ? undefined : true}
            className={cn(
              "transition-opacity duration-150",
              pausable ? "opacity-100" : "pointer-events-none opacity-0",
            )}
          >
            {t("cloudAfkAuto.pause")}
          </Button>
        )}
      </div>

      <section aria-label={t(`cloudAfkAuto.${mode}.items`)} className="flex flex-col gap-2">
        <Heading>{t(`cloudAfkAuto.${mode}.items`)}</Heading>
        {items.length ? (
          <ul className="flex flex-wrap gap-1.5">
            {items.map((item) => (
              <ItemCount key={item.id} id={item.id} n={item.n} name={name(item.id)} />
            ))}
          </ul>
        ) : (
          <p className="text-[12.5px] text-ash-400">{t(`cloudAfkAuto.${mode}.none`)}</p>
        )}
      </section>

      {lastCatches.length ? (
        <section className="flex flex-col gap-1.5 border-t border-white/7 pt-3">
          <Heading>{t("cloudAfkAuto.fishing.last")}</Heading>
          <ul className="grid grid-cols-1 gap-x-6 gap-y-1 @min-[460px]/auto:grid-cols-2">
            {lastCatches.map((caught, index) => {
              const treasure = caught.cat === "treasure";
              return (
                <li
                  key={`${caught.id}-${caught.at ?? index}`}
                  className="flex min-h-8 items-center gap-2.5"
                >
                  <ItemTile id={caught.id} size={24} />
                  <span className="min-w-0 flex-1 truncate text-[13px]">
                    <span className={treasure ? "text-gold-200" : "text-bone-100"}>
                      {name(caught.id)}
                    </span>
                    {caught.ench ? (
                      <span className="text-ash-400"> · {t("cloudAfkAuto.enchanted")}</span>
                    ) : null}
                    {treasure ? (
                      <span className="text-ash-400"> · {t("cloudAfkAuto.fishing.treasure")}</span>
                    ) : null}
                  </span>
                  {caught.at ? (
                    <span className="shrink-0 text-[12px] text-ash-400 tabular-nums">
                      {time.format(new Date(caught.at * 1000))}
                    </span>
                  ) : null}
                </li>
              );
            })}
          </ul>
        </section>
      ) : null}

      <section className="flex flex-col gap-3 border-t border-white/7 pt-3">
        <div className="flex items-center gap-3">
          {tool ? (
            <ToolTile item={tool.item || MODE_ITEM[mode]} dur={tool.dur} max={tool.max} />
          ) : (
            <span className="opacity-45">
              <ItemTile id={MODE_ITEM[mode]} size={40} />
            </span>
          )}
          <div className="min-w-0">
            <p className="truncate text-[13px] text-bone-100">
              {tool ? name(tool.item || MODE_ITEM[mode]) : t(`cloudAfkAuto.${mode}.tool`)}
            </p>
            <p className="text-[12.5px] text-ash-400 tabular-nums">
              {tool && tool.max > 0
                ? t("cloudAfkAuto.durability", { dur: tool.dur, max: tool.max })
                : t(
                    mode === "fishing"
                      ? "cloudAfkAuto.fishing.noTool"
                      : "cloudAfkAuto.mining.noToolHeld",
                  )}
            </p>
          </div>
        </div>
        {target && targetId ? (
          <div className="flex items-center gap-3">
            <ItemTile id={targetId} size={40} />
            <div className="min-w-0">
              <p className="truncate text-[13px] text-bone-100">
                {t("cloudAfkAuto.mining.lookingAt", { name: name(targetId) })}
              </p>
              <p className="text-[12.5px] text-ash-400 tabular-nums">
                X {target.x ?? 0} · Y {target.y ?? 0} · Z {target.z ?? 0}
              </p>
            </div>
          </div>
        ) : null}
      </section>

      <section
        aria-label={t("cloudAfkAuto.aim.title")}
        className="flex flex-col gap-2.5 border-t border-white/7 pt-3"
      >
        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
          <Heading>{t("cloudAfkAuto.aim.title")}</Heading>
          <fieldset
            aria-label={t("cloudAfkAuto.aim.title")}
            className="m-0 flex gap-1.5 border-0 p-0"
          >
            {(["north", "east", "south", "west"] as const).map((dir) => (
              <WalkButton
                key={dir}
                label={t(`cloudAfkAuto.aim.${dir}`)}
                ariaLabel={t(`cloudAfkAuto.aim.${dir}`)}
                pressed={aim.dir === dir}
                disabled={!aimable}
                onClick={() => aimAt({ ...aim, dir })}
              />
            ))}
          </fieldset>
        </div>
        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
          <span className="text-[13px] font-medium text-bone-100">
            {t("cloudAfkAuto.aim.look")}
            {aim.dir === null && aimable ? (
              <span className="ml-2 font-normal text-ash-400">{t("cloudAfkAuto.aim.first")}</span>
            ) : null}
          </span>
          <fieldset
            aria-label={t("cloudAfkAuto.aim.look")}
            className="m-0 flex gap-1.5 border-0 p-0"
          >
            {(["up", "level", "down"] as const).map((tilt) => (
              <WalkButton
                key={tilt}
                label={t(`cloudAfkAuto.aim.${tilt}`)}
                ariaLabel={t(`cloudAfkAuto.aim.${tilt}`)}
                pressed={aim.dir !== null && aim.tilt === tilt}
                disabled={!aimable || aim.dir === null}
                onClick={() => aimAt({ ...aim, tilt })}
              />
            ))}
          </fieldset>
        </div>
      </section>

      <Disclaimer />
    </div>
  );
}
