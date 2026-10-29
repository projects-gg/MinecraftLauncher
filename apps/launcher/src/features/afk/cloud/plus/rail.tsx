// The AFK+ panel's left column, the site's rail in the launcher's words: the assistant's state in
// one sentence with its one control (Durdur / Başlat), the sections with what waits in them (bone
// figures, no badges: the gold is the panel's save button's), and the AI allowance: two bars, when
// each renews, about how many replies are left. The bars move only when a number changes.
import { ArrowSquareOut, Play, Stop } from "@phosphor-icons/react";
import { Button, cn, enter, leave, toast, usePressGuard } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { formatCount, formatPercent } from "@/lib/format";
import { afkFailed } from "../../errors";
import type { CloudAccount } from "../api";
import { cloudApi } from "../api";
import type { PlusPanel, PlusQuota } from "./api";
import { LimitLine } from "./limits-link";
import {
  aiState,
  boughtInUse,
  boughtReplies,
  giftReplies,
  spendingField,
  startState,
  tradeNoteKey,
} from "./model";
import { atPhrase, clockText, dayText, untilPhrase } from "./time";
import { plusAi } from "./types";
import {
  type PlusFeatureFlags,
  type PlusSection,
  plusKey,
  usePlusControl,
  usePlusFeatures,
  usePlusTopupOpen,
  usePlusUi,
} from "./use-plus";
import { currentLang, trWith } from "./words";

export const SECTIONS: PlusSection[] = ["feed", "messages", "orders", "settings", "report"];

/** The sections Studio has not switched off (Mesajlar is QoL Q1, Rapor Q4). */
export function visibleSections(features: PlusFeatureFlags): PlusSection[] {
  return SECTIONS.filter(
    (section) =>
      (section !== "messages" || features.inbox) && (section !== "report" || features.report),
  );
}

/** The colour of the assistant's state sentence: never gold, a sentence is not an action (flare for an error). */
export const AI_TONE = {
  ok: "text-bone-50",
  muted: "text-ash-300",
  warn: "text-bone-100",
  error: "text-flare-300",
} as const;
const TONE = AI_TONE;

/** The assistant's state in one sentence, with its "when" in the language's own phrase. */
/**
 * The assistant's state in one sentence. `afterLabel`: the line follows the "AFK+" label (the
 * account's card), so a state that names AFK+ says it without the name ("AFK+ Bakımda").
 */
export function useAiSentence(
  account: CloudAccount,
  afterLabel = false,
): { text: string; tone: keyof typeof TONE } {
  const { t } = useTranslation();
  const state = aiState(plusAi(account));
  const lang = currentLang();
  const when = state.until
    ? state.until.how === "at"
      ? atPhrase(state.until.at, Date.now(), lang, t("afkPlus.time.tomorrow"))
      : untilPhrase(state.until.at, Date.now(), lang)
    : undefined;
  const text = t(`afkPlus.ai.${state.key}`, { when });
  return {
    text: afterLabel ? t(`afkPlus.ai.line.${state.key}`, { when, defaultValue: text }) : text,
    tone: state.tone,
  };
}

function StateBlock({
  account,
  ready = false,
  onSetup,
}: {
  account: CloudAccount;
  /** The service's `ready`: the assistant waits for its first start (it never starts by itself). */
  ready?: boolean;
  onSetup?: () => void;
}) {
  const { t } = useTranslation();
  const sentence = useAiSentence(account);
  const own = aiState(plusAi(account));
  // Waiting for its first start: one plain line and "ProBot AI'ı başlat" (a bare "Başlat" read as
  // the account's, beside its "Durdur"); after it, the state as ever.
  const state = startState(own, ready);
  const waits = state.key === "ready";
  const text = waits ? t("afkPlus.ai.ready") : sentence.text;
  const tone = waits ? state.tone : sentence.tone;
  // Out of credits only the AI stops: say what still works (cloud-afk README "AFK+ quotas").
  const stateNote =
    tradeNoteKey(plusAi(account)) ?? (state.key.startsWith("quota") ? "quotaNote" : null);
  const note = waits ? null : stateNote;
  const control = usePlusControl(account.id);
  const action = state.canStop ? "stop" : state.canStart ? "start" : null;
  const guard = usePressGuard(action ?? "none");
  const queryClient = useQueryClient();
  const goToLimit = usePlusUi((s) => s.goToLimit);
  // Buying waits for the spending limits: "Sınırlara git" to the first one missing, as last read.
  const goSpend = () => {
    const panel = queryClient.getQueryData<PlusPanel>(plusKey(account.id));
    goToLimit(panel ? spendingField(panel.settings.config.limits) : "perDealMax");
  };
  if (onSetup) {
    // Never set up: the one thing to do is the setup (the switch would start it with no limits;
    // the setup's last step is "Başlat"). While the machine can't be reached that is said instead
    // of "running with the defaults" (the setup is saved by the Worker and reaches the machine
    // later); a service that waits for the first start says it is ready, not running.
    const away = own.key === "unknown";
    return (
      <div className="flex flex-col gap-2.5">
        <p
          className={cn(
            "text-[13.5px] leading-snug font-medium text-pretty",
            away ? TONE[sentence.tone] : "text-bone-50",
          )}
        >
          {away ? sentence.text : waits ? text : t("afkPlus.ai.setup")}
        </p>
        <Button size="sm" variant="primary" onClick={onSetup} className="self-start">
          {t("afkPlus.setup.open")}
        </Button>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-2.5">
      <p
        aria-live="polite"
        className={cn(
          "text-[13.5px] leading-snug font-medium text-pretty transition-colors duration-150",
          TONE[tone],
        )}
      >
        {text}
      </p>
      {note ? (
        <LimitLine
          onGo={note === "notifyOnly" ? goSpend : null}
          className="-mt-1 text-[12px] leading-snug text-ash-400"
        >
          {t(`afkPlus.ai.${note}`)}
        </LimitLine>
      ) : null}
      {action ? (
        <Button
          size="sm"
          // Never gold on a card (one gold per screen, design.md §2): a start that waits for the
          // player (the first one, after a safety stop or a server mute) is a bone plate; their own
          // stop, and a start after it, are quiet.
          variant={action === "start" && state.tone !== "muted" ? "bone" : "secondary"}
          // A stop, not a pause: it stays off until the player starts it again (the README's `player`).
          icon={
            action === "stop" ? <Stop size={14} weight="fill" /> : <Play size={14} weight="fill" />
          }
          aria-label={t(
            action === "stop" ? "afkPlus.control.stopLabel" : "afkPlus.control.startLabel",
          )}
          onClick={(event) => {
            if (!guard.take(event)) return;
            void control(action === "stop" ? "pause" : "resume").then((done) => {
              if (done) {
                toast.success(
                  t(action === "stop" ? "afkPlus.control.stopped" : "afkPlus.control.started"),
                );
              }
            });
          }}
          {...guard.handlers}
          className="self-start"
        >
          {t(action === "stop" ? "afkPlus.control.stop" : "afkPlus.control.start")}
        </Button>
      ) : null}
    </div>
  );
}

function Nav({
  section,
  onSection,
  approvals,
  messages,
  orders,
  settingsDirty,
}: {
  section: PlusSection;
  onSection: (section: PlusSection) => void;
  approvals: number;
  messages: number;
  orders: number;
  settingsDirty: boolean;
}) {
  const { t } = useTranslation();
  const features = usePlusFeatures();
  const counts: Partial<Record<PlusSection, number>> = { feed: approvals, messages, orders };
  return (
    <nav aria-label={t("afkPlus.panel.sectionsLabel")}>
      <ul className="space-y-0.5">
        {visibleSections(features).map((value) => {
          const on = value === section;
          const count = counts[value] ?? 0;
          return (
            <li key={value}>
              <button
                type="button"
                aria-current={on ? "page" : undefined}
                onClick={() => onSection(value)}
                className={cn(
                  "relative isolate flex h-[34px] w-full cursor-pointer items-center justify-between gap-2 rounded-md px-2.5 text-left text-[13px] font-medium outline-none",
                  "transition-[color,background-color,transform,scale] duration-150 ease-out active:scale-[0.98] focus-visible:ring-2 focus-visible:ring-gold-300",
                  on ? "text-bone-50" : "text-ash-300 hover:bg-white/5 hover:text-bone-50",
                )}
              >
                {on ? (
                  <motion.span
                    aria-hidden
                    layoutId="afk-plus-section"
                    className="absolute inset-0 -z-10 rounded-md bg-white/8"
                    transition={{ type: "spring", stiffness: 480, damping: 40 }}
                  />
                ) : null}
                <span className="truncate">{t(`afkPlus.panel.sections.${value}`)}</span>
                {count > 0 ? (
                  <span className="text-[12.5px] font-semibold text-bone-100 tabular-nums">
                    {count}
                  </span>
                ) : null}
                {/* Unsaved settings wait: the tabs' small gold dot (design.md G6-P). */}
                <AnimatePresence initial={false}>
                  {value === "settings" && settingsDirty ? (
                    <motion.span
                      key="dirty"
                      className="pointer-events-none flex"
                      initial={{ opacity: 0, transform: "scale(0.6)" }}
                      animate={{ opacity: 1, transform: "scale(1)", transition: enter(0.2) }}
                      exit={{ opacity: 0, transform: "scale(0.6)", transition: leave(0.2) }}
                    >
                      <span aria-hidden className="size-1.5 rounded-full bg-gold-400" />
                      <span className="sr-only">{t("afkPlus.settings.dirty")}</span>
                    </motion.span>
                  ) : null}
                </AnimatePresence>
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

/** One window's bar: the used share as a still gold fill (no sheen: nothing here is working). */
function Bar({ label, pct }: { label: string; pct: number }) {
  const value = Math.max(0, Math.min(100, pct));
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(value)}
      className="relative h-1 overflow-hidden rounded-full bg-white/8"
    >
      <div
        className={cn(
          "absolute inset-0 rounded-full transition-transform duration-500 ease-[cubic-bezier(0.16,1,0.3,1)]",
          value >= 100
            ? "bg-gold-300"
            : "bg-[linear-gradient(90deg,var(--color-gold-600),var(--color-gold-400))]",
        )}
        style={{ transform: `translateX(${value - 100}%)` }}
      />
    </div>
  );
}

function Window({ title, pct, resetAt }: { title: string; pct: number; resetAt: number | null }) {
  const { t } = useTranslation();
  const lang = currentLang();
  return (
    <div className="flex flex-col gap-1.5">
      <p className="flex items-baseline justify-between gap-2 text-[12.5px]">
        <span className="text-bone-100">{title}</span>
        <span className="text-ash-300 tabular-nums">
          {t("afkPlus.quota.used", { percent: formatPercent(pct) })}
        </span>
      </p>
      <Bar label={title} pct={pct} />
      <p className="text-[12px] text-ash-400">
        {resetAt
          ? upperFirst(
              t("afkPlus.quota.resets", {
                when: atPhrase(resetAt, Date.now(), lang, t("afkPlus.time.tomorrow")),
              }),
              lang,
            )
          : t("afkPlus.quota.notStarted")}
      </p>
    </div>
  );
}

/** A line that starts with a phrase ("yarın 04:00'te yenilenir") starts with a capital. */
function upperFirst(text: string, lang: string): string {
  return text.charAt(0).toLocaleUpperCase(lang === "tr" ? "tr" : "en") + text.slice(1);
}

/** A date phrase that never breaks between its day and month ("31 Ekim'e kadar"). */
const unbroken = (text: string) => text.replace(/ /g, " ");

/**
 * The gift's line under "about N replies left": all of them, or about how many, and the day the
 * first one ends (the site's own words: "Yaklaşık 40 tanesi hediye, 1 Kasım'a kadar geçerli").
 */
function giftLine(quota: PlusQuota, t: TFunction, lang: "tr" | "en"): string | null {
  const count = giftReplies(quota);
  if (count <= 0) return null;
  const end = quota.gifts.find((g) => g.expiresAt !== null)?.expiresAt ?? null;
  const which = count >= quota.estReplies ? "giftAll" : "giftSome";
  if (!end) return t(`afkPlus.quota.${which}`, { count });
  const day = dayText(end, lang);
  return t(`afkPlus.quota.${which}Until`, {
    count,
    day: unbroken(lang === "tr" ? trWith(day, "dat") : day),
  });
}

function QuotaBlock({
  quota,
  saverAt,
  account,
}: {
  quota: PlusQuota;
  saverAt: number | null;
  account: CloudAccount;
}) {
  const { t } = useTranslation();
  const lang = currentLang();
  // AFK+ extra credits (cloud-afk README "AFK+ extra credits"): how many of the replies the bought
  // credits pay for, in the gift's words; once the windows and the gifts are spent they pay for all
  // of them, and the line says they are in use now (the site's same lines).
  const boughtNow = boughtInUse(quota);
  const fromBought = !quota.frozen && quota.estReplies > 0 ? boughtReplies(quota) : 0;
  const canBuy = usePlusTopupOpen() && account.tier === "plus" && !quota.frozen;
  // Only under the replies line: a frozen or used-up allowance has no "of them".
  const gift = !quota.frozen && quota.estReplies > 0 ? giftLine(quota, t, lang) : null;
  const stale = Date.now() / 1000 - quota.asOf > 90;
  // Past the player's share (README "Saving"): small talk and chat scans stop, deals go on.
  const used = Math.max(quota.pct5, quota.pctWeek);
  const saving = saverAt !== null && used >= saverAt && used < 100;
  return (
    <div className="flex flex-col gap-3">
      <Window title={t("afkPlus.quota.w5h")} pct={quota.pct5} resetAt={quota.w5h.resetAt} />
      <Window title={t("afkPlus.quota.week")} pct={quota.pctWeek} resetAt={quota.week.resetAt} />
      <div className="flex flex-col gap-1 text-[12.5px]">
        {quota.frozen ? (
          <p className="text-flare-300">{t("afkPlus.quota.frozen")}</p>
        ) : quota.estReplies > 0 ? (
          // Used up, the state sentence above says so; "about 0 replies" would say nothing.
          <p className="text-bone-100 tabular-nums">
            {t("afkPlus.quota.replies", { count: quota.estReplies })}
          </p>
        ) : null}
        {gift ? <p className="-mt-0.5 text-[12px] text-ash-300 tabular-nums">{gift}</p> : null}
        {boughtNow ? (
          <p className="-mt-0.5 text-[12px] text-bone-100">{t("afkPlus.quota.boughtInUse")}</p>
        ) : fromBought > 0 ? (
          <p className="-mt-0.5 text-[12px] text-ash-300 tabular-nums">
            {t("afkPlus.quota.bought", { count: fromBought, n: formatCount(fromBought) })}
          </p>
        ) : null}
        {/* A promotion is news, not an action: plain words, no gold. */}
        {quota.promo ? (
          <p className="text-bone-100">
            {lang === "en" ? quota.promo.label.en : quota.promo.label.tr}
          </p>
        ) : null}
        {saving ? <p className="text-ash-300">{t("afkPlus.quota.saver")}</p> : null}
        {stale ? (
          <p className="text-ash-400">
            {t("afkPlus.quota.asOf", { time: clockText(quota.asOf, lang) })}
          </p>
        ) : null}
      </div>
      {/* Buying happens on afk.projects.gg: the account's AFK+ tab with the packs open. */}
      {canBuy ? (
        <Button
          variant="secondary"
          size="sm"
          // The site opens: the open-in-browser mark after the words, as "afk.projects.gg'de aç".
          iconRight={<ArrowSquareOut size={14} weight="bold" />}
          className="self-start"
          onClick={() =>
            void cloudApi
              .open("plusTopup", account.id)
              .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")))
          }
        >
          {t("afkPlus.quota.buy")}
        </Button>
      ) : null}
    </div>
  );
}

export function PlusRail({
  account,
  section,
  onSection,
  approvals,
  messages,
  orders,
  quota,
  saverAt = null,
  settingsDirty = false,
  ready = false,
  onSetup,
}: {
  account: CloudAccount;
  section: PlusSection;
  onSection: (section: PlusSection) => void;
  approvals: number;
  messages: number;
  /** Orders that wait for the player (an OK, a trade to make). */
  orders: number;
  /** Null while the machine cannot be asked: the state line says so once, the block is not shown. */
  quota: PlusQuota | null;
  /** The share of the allowance after which small talk stops (Studio's `saver` on), or null. */
  saverAt?: number | null;
  /** "Ayarlar" holds unsaved edits. */
  settingsDirty?: boolean;
  /** The assistant waits for its first start (the service's `ready`). */
  ready?: boolean;
  /** The account was never set up: the state block offers the setup instead of the switch. */
  onSetup?: () => void;
}) {
  const { t } = useTranslation();
  return (
    <aside className="flex w-[232px] shrink-0 flex-col gap-5 overflow-y-auto border-r border-white/7 px-4 py-4 [scrollbar-width:thin]">
      <StateBlock account={account} ready={ready} onSetup={onSetup} />
      <div className="border-t border-white/7 pt-4">
        <Nav
          section={section}
          onSection={onSection}
          approvals={approvals}
          messages={messages}
          orders={orders}
          settingsDirty={settingsDirty}
        />
      </div>
      {quota ? (
        <section aria-label={t("afkPlus.quota.title")} className="border-t border-white/7 pt-4">
          <h3 className="mb-3 font-display text-[14px] font-semibold text-bone-50">
            {t("afkPlus.quota.title")}
          </h3>
          <QuotaBlock quota={quota} saverAt={saverAt} account={account} />
        </section>
      ) : null}
    </aside>
  );
}
