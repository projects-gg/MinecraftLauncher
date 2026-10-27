// 7/24 Bulut on the AFK page, under the desktop accounts. Nothing of it renders unless the service
// answered and is not off: with the service down the page is exactly what it was. Without a cloud
// account (or signed out of Friends) it is one quiet intro card; with accounts, their cards. The
// layers (intro, the account panel, settings, "Buluta taşı", the two-places question) live here
// too. With a service of the cloud features and two accounts or more, "Seç" chooses several for
// one action at a time (CAFK-FEAT §3.4).
import { ArrowSquareOut, ArrowsClockwise, WarningCircle } from "@phosphor-icons/react";
import { Button, cn, enter, IconButton, leave, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AfkAccount } from "@/lib/api";
import { afkFailed } from "../errors";
import { cloudApi } from "./api";
import { BulkBar } from "./bulk-bar";
import { CloudCard } from "./card";
import { CloudConsole } from "./console";
import { CopyDialog } from "./copy-dialog";
import { CloudGuardDialog } from "./guard";
import { CloudIntroCard, CloudIntroDialog } from "./intro";
import { CloudMoveDialog } from "./move";
import { PlusIntroCard, PlusIntroDialog } from "./plus/intro";
import { PlusPanel } from "./plus/plus-panel";
import { plusMe, plusOffer } from "./plus/types";
import { isPlusAccount, plusShown, usePlusUi } from "./plus/use-plus";
import { CloudSettingsSheet } from "./settings";
import { localized } from "./status";
import {
  useCloudAction,
  useCloudPage,
  useCloudSync,
  useCloudUi,
  useGuardedStart,
} from "./use-cloud";

/** "Ayarları kopyala": where the settings may come from and where they go, as the caller chose. */
interface CopyRequest {
  source: string | null;
  sources: string[];
  targets: string[];
}

/** The bulk bar's height, its gap to the bottom and some air above it. */
const BAR_ROOM = 96;

/** The nearest ancestor that scrolls vertically (the AFK page's scroll area). */
function scrollParentOf(node: HTMLElement | null): HTMLElement | null {
  for (let element = node?.parentElement ?? null; element; element = element.parentElement) {
    const { overflowY } = getComputedStyle(element);
    if (overflowY === "auto" || overflowY === "scroll") return element;
  }
  return null;
}

/** Scrolls the cards' end clear of the bulk bar, as far as the section's header stays in view. */
function revealChosen(list: HTMLElement | null, head: HTMLElement | null, reduce: boolean) {
  const scroller = scrollParentOf(list);
  if (!list || !scroller) return;
  const view = scroller.getBoundingClientRect();
  const covered = list.getBoundingClientRect().bottom - (view.bottom - BAR_ROOM);
  const room = head ? head.getBoundingClientRect().top - view.top : covered;
  const by = Math.min(covered, Math.max(0, room));
  if (by > 0) scroller.scrollBy({ top: by, behavior: reduce ? "auto" : "smooth" });
}

export function CloudSection({
  desktop,
  centered = false,
}: {
  /** The desktop AFK accounts (the guard and "Buluta taşı" read them). */
  desktop: AfkAccount[];
  /** The page has no desktop account yet: the intro sits under its centred form. */
  centered?: boolean;
}) {
  const { t, i18n } = useTranslation();
  useCloudSync();
  const { data, skewMs, refetch, isFetching } = useCloudPage();
  const ui = useCloudUi();
  const stop = useCloudAction("stop");
  const restart = useCloudAction("restart");
  const guardedStart = useGuardedStart();
  const openPlus = usePlusUi((s) => s.openPanel);
  const [plusIntro, setPlusIntro] = useState(false);
  const [selecting, setSelecting] = useState(false);
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [copy, setCopy] = useState<CopyRequest | null>(null);
  // "Seç" and "Hepsini seç" swap places: the focus follows from one to the other.
  const selectRef = useRef<HTMLButtonElement>(null);
  const allRef = useRef<HTMLButtonElement>(null);
  const moveFocus = useRef(false);
  const listRef = useRef<HTMLUListElement>(null);
  const headRef = useRef<HTMLElement>(null);
  const { reduce } = useMotionPrefs();
  const overview = data?.overview;
  const listed = overview?.session === "signedIn" ? overview.accounts : [];
  // Several accounts at once: a service of the cloud features, and something to choose among.
  const bulk = Boolean(overview?.features) && listed.length >= 2;

  // Choosing ends with Escape (when no layer is open), or when bulk management goes.
  useEffect(() => {
    if (!selecting) return;
    if (!bulk) {
      setSelecting(false);
      return;
    }
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      if (document.querySelector("[role=dialog], [role=menu]")) return;
      moveFocus.current = true;
      setSelecting(false);
      setChosen(new Set());
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selecting, bulk]);

  useEffect(() => {
    if (!moveFocus.current) return;
    moveFocus.current = false;
    (selecting ? allRef : selectRef).current?.focus({ preventScroll: true });
  }, [selecting]);

  // The bar rises over the last row of cards when the first one is chosen: the list glides up clear
  // of it (the bar's spacer leaves the room), but never past the section's own header, where
  // "Hepsini seç" and "Vazgeç" are.
  const barOpen = selecting && chosen.size > 0;
  useEffect(() => {
    if (!barOpen) return;
    const frame = requestAnimationFrame(() =>
      revealChosen(listRef.current, headRef.current, reduce),
    );
    return () => cancelAnimationFrame(frame);
  }, [barOpen, reduce]);

  // Accounts that went (deleted elsewhere) leave the choice.
  const listedIds = listed.map((account) => account.id).join("|");
  useEffect(() => {
    setChosen((current) => {
      const ids = new Set(listedIds.split("|"));
      const next = new Set([...current].filter((id) => ids.has(id)));
      return next.size === current.size ? current : next;
    });
  }, [listedIds]);

  if (!overview?.available) return null;

  const lang = i18n.language === "en" ? "en" : "tr";
  const signedIn = overview.session === "signedIn";
  const failed = overview.session === "failed";
  const accounts = signedIn ? overview.accounts : [];
  const online = accounts.filter((a) => a.live.state === "running" && !a.hold).length;
  const maintenance =
    overview.mode === "paused"
      ? (localized(overview.maintenance, lang) ?? t("cloudAfk.maintenance"))
      : null;
  const consoleAccount = accounts.find((a) => a.id === ui.consoleId);
  const settingsAccount = accounts.find((a) => a.id === ui.settingsId);
  const moveDesktop = desktop.find((d) => d.id === ui.moveFor);
  // Getting a new account starts on the site's front page, which explains the offer (owner, 23 Oct).
  const buy = () =>
    cloudApi
      .open("start")
      .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")));
  // AFK+ shows only while the service offers it (README: "off" means invisible).
  const plus = plusShown(data);
  const canBuyPlus =
    plus && plusMe(overview)?.canBuy.ok !== false && plusOffer(overview)?.sales !== false;
  const plusIntroCard = plus && !accounts.some(isPlusAccount) && canBuyPlus;
  const upgrade = (id: string) =>
    cloudApi
      .open("plusBuy", id)
      .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")));
  const chosenAccounts = accounts.filter((account) => chosen.has(account.id));
  const choose = (id: string, on: boolean) =>
    setChosen((current) => {
      const next = new Set(current);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  const allChosen = accounts.length > 0 && chosenAccounts.length === accounts.length;
  const stopChoosing = () => {
    moveFocus.current = true;
    setSelecting(false);
    setChosen(new Set());
  };
  const startChoosing = () => {
    moveFocus.current = true;
    setSelecting(true);
  };

  return (
    <section
      aria-label={t("cloudAfk.title")}
      className={cn("mt-7", centered && "mx-auto w-full max-w-[460px]")}
    >
      {accounts.length > 0 ? (
        <header
          ref={headRef}
          className="mb-3 flex flex-wrap items-end justify-between gap-3 border-t border-white/7 pt-5"
        >
          <div>
            <h2 className="font-display text-section font-semibold text-bone-50">
              {t("cloudAfk.title")}
            </h2>
            <p className="mt-0.5 text-[13px] text-ash-300 tabular-nums">
              {online > 0
                ? t("cloudAfk.countOnline", { count: accounts.length, online })
                : t("cloudAfk.count", { count: accounts.length })}
            </p>
          </div>
          <div className="flex items-center gap-2">
            {bulk ? (
              selecting ? (
                <>
                  <Button
                    ref={allRef}
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      setChosen(allChosen ? new Set() : new Set(accounts.map((a) => a.id)))
                    }
                  >
                    {t("cloudAfkFeat.bulk.selectAll")}
                  </Button>
                  <Button size="sm" variant="ghost" onClick={stopChoosing}>
                    {t("cloudAfkFeat.bulk.cancel")}
                  </Button>
                </>
              ) : (
                <Button ref={selectRef} size="sm" variant="ghost" onClick={startChoosing}>
                  {t("cloudAfkFeat.bulk.select")}
                </Button>
              )
            ) : null}
            <IconButton
              label={t("cloudAfk.refresh")}
              icon={<ArrowsClockwise size={16} />}
              size="sm"
              disabled={isFetching}
              onClick={() => void refetch()}
            />
            {overview.salesOpen && !overview.soldOut ? (
              <Button
                variant="ghost"
                icon={<ArrowSquareOut size={15} weight="bold" />}
                onClick={() => void buy()}
              >
                {t("cloudAfk.buy")}
              </Button>
            ) : null}
          </div>
        </header>
      ) : null}

      <AnimatePresence initial={false}>
        {maintenance ? (
          <motion.p
            key="maintenance"
            role="status"
            className="mb-3 flex items-start gap-2 rounded-[var(--radius-panel)] bg-gold-500/10 px-3.5 py-2.5 text-[12.5px] text-gold-200 ring-1 ring-gold-400/20"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.14) }}
          >
            <WarningCircle size={15} weight="fill" className="mt-px shrink-0" aria-hidden />
            <span>{maintenance}</span>
          </motion.p>
        ) : null}
      </AnimatePresence>

      {failed ? (
        <div className="flex flex-wrap items-center gap-3 text-[13px] text-ash-300">
          <span>{t("cloudAfk.loadFailed")}</span>
          <Button
            size="sm"
            variant="secondary"
            disabled={isFetching}
            onClick={() => void refetch()}
          >
            {t("common.retry")}
          </Button>
        </div>
      ) : accounts.length > 0 ? (
        <ul ref={listRef} className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-3">
          <AnimatePresence initial={false} mode="popLayout">
            {accounts.map((account, index) => (
              <CloudCard
                key={account.id}
                account={account}
                skewMs={skewMs}
                index={index}
                onStart={() => void guardedStart(account, desktop)}
                onStop={() => void stop(account)}
                onRestart={() => void restart(account)}
                onConsole={() => ui.openConsole(account.id)}
                onSettings={() => ui.openSettings(account.id)}
                onPlus={plus && isPlusAccount(account) ? () => openPlus(account.id) : undefined}
                onUpgrade={
                  canBuyPlus && !isPlusAccount(account) && !account.expired
                    ? () => void upgrade(account.id)
                    : undefined
                }
                onInventory={
                  (account.features?.inventory?.mode ?? "off") !== "off" && account.host
                    ? () => ui.openConsole(account.id, "inventory")
                    : undefined
                }
                onCopy={
                  bulk
                    ? () =>
                        setCopy({
                          source: account.id,
                          sources: [account.id],
                          targets: chosenAccounts
                            .map((one) => one.id)
                            .filter((id) => id !== account.id),
                        })
                    : undefined
                }
                selecting={selecting}
                selected={chosen.has(account.id)}
                onSelect={(on) => choose(account.id, on)}
              />
            ))}
          </AnimatePresence>
        </ul>
      ) : plusIntroCard ? (
        // Two offers side by side (stacked under the page's centred form).
        <div className={cn("grid gap-3", centered ? "grid-cols-1" : "max-w-[940px] grid-cols-2")}>
          <CloudIntroCard overview={overview} onOpen={() => ui.setIntroOpen(true)} />
          <PlusIntroCard overview={overview} onOpen={() => setPlusIntro(true)} />
        </div>
      ) : (
        <CloudIntroCard
          overview={overview}
          onOpen={() => ui.setIntroOpen(true)}
          className={centered ? undefined : "max-w-[460px]"}
        />
      )}

      <CloudIntroDialog overview={overview} open={ui.introOpen} onOpenChange={ui.setIntroOpen} />
      {plus ? (
        <>
          <PlusIntroDialog overview={overview} open={plusIntro} onOpenChange={setPlusIntro} />
          <PlusPanel />
        </>
      ) : null}
      {bulk && selecting ? (
        <BulkBar
          open={selecting}
          chosen={chosenAccounts}
          desktop={desktop}
          onCopy={() =>
            setCopy({
              source: chosenAccounts[0]?.id ?? null,
              sources: chosenAccounts.map((one) => one.id),
              targets: chosenAccounts.slice(1).map((one) => one.id),
            })
          }
        />
      ) : null}
      {bulk ? (
        <CopyDialog
          open={copy !== null}
          accounts={accounts}
          sources={accounts.filter((account) => copy?.sources.includes(account.id))}
          initialSource={copy?.source ?? null}
          initialTargets={copy?.targets ?? []}
          onClose={() => setCopy(null)}
        />
      ) : null}
      <CloudConsole
        account={consoleAccount}
        skewMs={skewMs}
        onStart={(account) => void guardedStart(account, desktop)}
        onRestart={(account) => void restart(account)}
        onOpenSettings={(id) => {
          ui.openConsole(null);
          ui.openSettings(id);
        }}
        onClose={() => ui.openConsole(null)}
      />
      <CloudSettingsSheet account={settingsAccount} onClose={() => ui.openSettings(null)} />
      <CloudMoveDialog
        desktop={moveDesktop}
        accounts={accounts}
        skewMs={skewMs}
        onClose={() => ui.openMove(null)}
        onOpenConsole={(id) => ui.openConsole(id)}
      />
      <CloudGuardDialog />
    </section>
  );
}
