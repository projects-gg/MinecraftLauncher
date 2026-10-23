// 7/24 Bulut on the AFK page, under the desktop accounts. Nothing of it renders unless the service
// answered and is not off: with the service down the page is exactly what it was. Without a cloud
// account (or signed out of Friends) it is one quiet intro card; with accounts, their cards. The
// layers (intro, console, settings, "Buluta taşı", the two-places question) live here too.
import { ArrowSquareOut, ArrowsClockwise, WarningCircle } from "@phosphor-icons/react";
import { Button, cn, enter, IconButton, leave } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import type { AfkAccount } from "@/lib/api";
import { afkFailed } from "../errors";
import { cloudApi } from "./api";
import { CloudCard } from "./card";
import { CloudConsole } from "./console";
import { CloudGuardDialog } from "./guard";
import { CloudIntroCard, CloudIntroDialog } from "./intro";
import { CloudMoveDialog } from "./move";
import { CloudSettingsSheet } from "./settings";
import { localized } from "./status";
import {
  useCloudAction,
  useCloudPage,
  useCloudSync,
  useCloudUi,
  useGuardedStart,
} from "./use-cloud";

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
  const overview = data?.overview;
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
    cloudApi.open("start").catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")));

  return (
    <section
      aria-label={t("cloudAfk.title")}
      className={cn("mt-7", centered && "mx-auto w-full max-w-[460px]")}
    >
      {accounts.length > 0 ? (
        <header className="mb-3 flex flex-wrap items-end justify-between gap-3 border-t border-white/7 pt-5">
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
        <ul className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-3">
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
              />
            ))}
          </AnimatePresence>
        </ul>
      ) : (
        <CloudIntroCard
          overview={overview}
          onOpen={() => ui.setIntroOpen(true)}
          className={centered ? undefined : "max-w-[460px]"}
        />
      )}

      <CloudIntroDialog overview={overview} open={ui.introOpen} onOpenChange={ui.setIntroOpen} />
      <CloudConsole
        account={consoleAccount}
        skewMs={skewMs}
        onStart={(account) => void guardedStart(account, desktop)}
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
