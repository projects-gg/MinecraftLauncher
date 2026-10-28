// The AFK+ panel: a right sheet like the account center (design.md §5 "Right sheets"). On the left
// the rail (the assistant's state and its one control, the sections with what waits in them, the
// AI allowance); on the right one section at a time, entering with the tab panels' 4 px rise. It
// reads once when it opens and after an action; while it is on screen its account streams
// (`usePlusLive`), and nothing runs once it closes. An account never set up opens the setup
// wizard over it; "Sonra" leaves the panel usable. Unsaved "Ayarlar" edits wait in the store, so
// a click outside may close the panel at any time.
import { ArrowSquareOut, CloudSlash, Wrench, X } from "@phosphor-icons/react";
import {
  Button,
  EntranceGate,
  focusPanel,
  GatedMotion,
  IconButton,
  Skeleton,
  scrimMotion,
  sheetMotion,
  sheetOutside,
  useInteractionMotion,
} from "@pl/ui";
import { AnimatePresence } from "motion/react";
import { Dialog } from "radix-ui";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AfkHead } from "../../afk-head";
import { afkFailed, isFriendly } from "../../errors";
import { type CloudAccount, cloudApi } from "../api";
import { serverLabel } from "../status";
import { useCloudPage } from "../use-cloud";
import { type PlusApproval, plusApi } from "./api";
import { useJoinForTrade } from "./approval-card";
import { useApprovalNow, waitsNow } from "./approval-clock";
import { AuditSection } from "./audit-section";
import { PlusItemArt } from "./item-icon";
import { type Namer, usePlusItemData } from "./items";
import { MessagesSection } from "./messages-section";
import { OrdersSection } from "./orders-section";
import { PlusRail, visibleSections } from "./rail";
import { ReportSection } from "./report-section";
import { SettingsSection } from "./settings-section";
import { PlusSetupWizard } from "./setup-wizard";
import { plusAi } from "./types";
import { isPlusAccount, usePlusFeatures, usePlusLive, usePlusPanel, usePlusUi } from "./use-plus";

const NONE: PlusApproval[] = [];

function PanelSkeleton() {
  return (
    <div className="flex min-h-0 flex-1" aria-hidden>
      <div className="flex w-[232px] shrink-0 flex-col gap-3 border-r border-white/7 px-4 py-4">
        <Skeleton className="h-4 w-36" />
        <Skeleton className="h-7 w-24" />
        <div className="mt-3 space-y-2 border-t border-white/7 pt-4">
          <Skeleton className="h-[30px] w-full" />
          <Skeleton className="h-[30px] w-full" />
          <Skeleton className="h-[30px] w-full" />
          <Skeleton className="h-[30px] w-full" />
          <Skeleton className="h-[30px] w-full" />
        </div>
      </div>
      <div className="flex-1 space-y-4 px-6 pt-6">
        <Skeleton className="h-5 w-40" />
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-4 w-[70%]" />
        <Skeleton className="h-4 w-[82%]" />
        <Skeleton className="h-4 w-[64%]" />
      </div>
    </div>
  );
}

/**
 * Said once above the section, in the site's words: the machine cannot be reached (what still
 * works and what waits for it), or AFK+ is under maintenance. A quiet box, never a coloured one.
 */
function PanelNotice({
  icon,
  title,
  children,
}: {
  icon: ReactNode;
  title: string;
  children: ReactNode;
}) {
  return (
    <div
      role="status"
      className="mb-5 flex items-start gap-3 rounded-[var(--radius-panel)] bg-white/[0.04] px-3.5 py-3 ring-1 ring-white/7 ring-inset"
    >
      <span aria-hidden className="mt-px shrink-0 text-ash-300">
        {icon}
      </span>
      <div className="min-w-0">
        <p className="text-[13.5px] font-medium text-bone-50">{title}</p>
        <p className="mt-0.5 text-[12.5px] text-pretty text-ash-300">{children}</p>
      </div>
    </div>
  );
}

function PanelBody({ account, onClose }: { account: CloudAccount; onClose: () => void }) {
  const { t } = useTranslation();
  const features = usePlusFeatures();
  const chosen = usePlusUi((s) => s.section);
  // A section Studio switched off meanwhile falls back to the feed.
  const section = visibleSections(features).includes(chosen) ? chosen : "feed";
  const setSection = usePlusUi((s) => s.setSection);
  const wizardFor = usePlusUi((s) => s.wizardFor);
  const setWizard = usePlusUi((s) => s.setWizard);
  const putOff = usePlusUi((s) => s.setupLater.includes(account.id));
  const putOffSetup = usePlusUi((s) => s.putOffSetup);
  const dirty = usePlusUi((s) => Boolean(s.drafts[account.id]));
  const query = usePlusPanel(account.id);
  const link = usePlusLive(account.id);
  const panel = query.data;
  const needsSetup = panel?.settings.aiRev === 0;
  const items = usePlusItemData(panel?.settings.server.mc ?? null);
  const namer: Namer = {
    data: items.data,
    custom: panel?.settings.config.customItems ?? [],
    unnamed: t("afkPlus.items.unnamed"),
  };
  const join = useJoinForTrade(account);
  // "Ayarlar"'s change bar docks here (change-dock.tsx).
  const [dock, setDock] = useState<HTMLDivElement | null>(null);

  // A first look at an account never set up: the wizard comes first, once per session.
  useEffect(() => {
    if (needsSetup && !putOff) setWizard(account.id);
  }, [needsSetup, putOff, account.id, setWizard]);

  // What still waits (its time not run out), as the feed and the rows count it.
  const now = useApprovalNow(panel?.approvals ?? NONE);
  const waitingNow = panel?.approvals.filter((a) => waitsNow(a, now)) ?? null;
  const pending = waitingNow?.length ?? account.ai?.pendingApprovals ?? 0;
  const unread = account.ai?.newMessages?.count ?? 0;
  const ai = plusAi(account);
  const away = Boolean(panel?.machineAway) || ai?.state === "unknown";
  const maintenance = ai?.reason === "service" && ai.state === "paused";
  const awayNotice =
    away && (section === "feed" || section === "messages" || section === "orders") ? (
      <PanelNotice icon={<CloudSlash size={18} />} title={t("afkPlus.panel.away.title")}>
        {[
          t("afkPlus.panel.away.body"),
          // The messages say their own line in their section (no blank pane under its title).
          pending && section !== "messages" ? t("afkPlus.panel.away.approvals") : null,
        ]
          .filter(Boolean)
          .join(" ")}
      </PanelNotice>
    ) : null;
  const serviceNotice =
    maintenance && (section === "feed" || section === "orders") ? (
      <PanelNotice icon={<Wrench size={18} />} title={t("afkPlus.panel.service.title")}>
        {t("afkPlus.panel.service.body")}
      </PanelNotice>
    ) : null;
  // The orders that wait for the player (an OK, a first sight, a trade to make): those with
  // something waiting under their row, as the rows say it.
  const waitingOrders = new Set((waitingNow ?? []).map((a) => a.order));
  const waiting = panel
    ? panel.settings.config.orders.filter((order) => waitingOrders.has(order.id)).length
    : 0;

  return (
    <>
      <header className="flex items-center gap-3.5 border-b border-white/8 px-5 py-4">
        <AfkHead nickname={account.name} size={36} />
        <div className="min-w-0 flex-1">
          <Dialog.Title className="truncate font-display text-[18px] font-semibold text-bone-50">
            {t("afkPlus.panel.title", { name: account.name })}
          </Dialog.Title>
          <p className="truncate text-[12.5px] text-ash-400" data-selectable>
            {serverLabel(account.server)}
          </p>
        </div>
        {/* The site's page of this account: here, always on screen (in the rail it fell below the
            fold at 1024x640). */}
        <button
          type="button"
          onClick={() =>
            void cloudApi
              .open("plusAccount", account.id)
              .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")))
          }
          className="inline-flex shrink-0 cursor-pointer items-center gap-1.5 rounded-sm text-[12.5px] font-medium text-ash-300 underline-offset-2 outline-none transition-colors duration-150 hover:text-gold-200 hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          {t("afkPlus.panel.site")}
          <ArrowSquareOut size={12} weight="bold" aria-hidden />
        </button>
        <IconButton label={t("common.close")} icon={<X size={18} />} size="sm" onClick={onClose} />
      </header>

      {panel ? (
        <div className="flex min-h-0 flex-1">
          <PlusRail
            account={account}
            section={section}
            onSection={setSection}
            approvals={pending}
            messages={unread}
            orders={waiting}
            quota={panel.quota}
            saverAt={features.saver ? (panel.settings.config.replies.saveAt ?? null) : null}
            settingsDirty={dirty}
            ready={panel.settings.config.ready === true}
            onSetup={needsSetup ? () => setWizard(account.id) : undefined}
          />
          <div
            data-plus-scroller
            className="min-w-0 flex-1 overflow-y-auto [scrollbar-gutter:stable] [scrollbar-width:thin]"
          >
            <div key={section} className="panel-in flex min-h-full flex-col px-6 pt-5 pb-6">
              {awayNotice ?? serviceNotice}
              {section === "feed" ? (
                <AuditSection
                  accountId={account.id}
                  approvals={panel.approvals}
                  namer={namer}
                  server={panel.settings.server}
                  link={link}
                  machineAway={panel.machineAway}
                  enabled={features.audit && panel.settings.rules.features.audit}
                  canLower={ai?.state === "on"}
                  maintenance={maintenance}
                  onJoin={join.ask}
                />
              ) : section === "messages" ? (
                <MessagesSection accountId={account.id} />
              ) : section === "orders" ? (
                <OrdersSection account={account} panel={panel} />
              ) : section === "settings" ? (
                <SettingsSection account={account} panel={panel} dock={dock} />
              ) : (
                <ReportSection account={account} namer={namer} server={panel.settings.server} />
              )}
            </div>
            {/* Stuck to the scroller's bottom edge, as wide as its content (not over the scroll bar). */}
            <div ref={setDock} className="sticky bottom-0 z-10 h-0" />
          </div>
        </div>
      ) : query.isError ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 p-10 text-center">
          <p className="font-display text-[17px] font-semibold text-bone-50">
            {t("afkPlus.panel.loadFailed")}
          </p>
          {isFriendly(query.error) ? (
            <p className="max-w-[46ch] text-[13px] text-ash-400">{query.error.message}</p>
          ) : null}
          <Button
            variant="secondary"
            className="mt-3"
            onClick={() => void query.refetch()}
            disabled={query.isFetching}
          >
            {t("common.retry")}
          </Button>
        </div>
      ) : (
        <PanelSkeleton />
      )}

      {join.dialog}
      <PlusSetupWizard
        account={account}
        panel={panel}
        open={wizardFor === account.id && Boolean(panel)}
        onLater={() => {
          putOffSetup(account.id);
          setWizard(null);
        }}
        onDone={() => {
          setWizard(null);
          setSection("feed");
        }}
      />
    </>
  );
}

/** The AFK+ panel of the account `usePlusUi().panelId` names, while that account is listed. */
export function PlusPanel() {
  const animate = useInteractionMotion();
  const { data } = useCloudPage();
  const panelId = usePlusUi((s) => s.panelId);
  const closePanel = usePlusUi((s) => s.closePanel);
  const account = data?.overview.accounts.find((a) => a.id === panelId);
  const open = Boolean(account);

  // The audit logs kept on this computer for accounts no longer on AFK+ go, once the accounts were
  // read (`now`: a failed read lists none and says nothing about them).
  const plusIds = data?.overview.now
    ? data.overview.accounts
        .filter(isPlusAccount)
        .map((a) => a.id)
        .sort()
        .join(" ")
    : null;
  useEffect(() => {
    if (plusIds === null) return;
    void plusApi.auditPrune(plusIds ? plusIds.split(" ") : []).catch(() => undefined);
  }, [plusIds]);

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && closePanel()}>
      <AnimatePresence>
        {open && account ? (
          <EntranceGate key="plus-panel">
            <Dialog.Portal forceMount>
              <Dialog.Overlay asChild forceMount>
                <GatedMotion.div
                  className="fixed inset-0 z-30 bg-obsidian-950/55"
                  {...scrimMotion(!animate)}
                />
              </Dialog.Overlay>
              <Dialog.Content
                asChild
                forceMount
                aria-describedby={undefined}
                // A press outside closes the panel (its "Ayarlar" edits wait); a toast's button,
                // such as an approval's, does not (SHEET-UX).
                {...sheetOutside}
                onOpenAutoFocus={(event) => {
                  event.preventDefault();
                  focusPanel(event.currentTarget as HTMLElement);
                }}
              >
                <GatedMotion.aside
                  tabIndex={-1}
                  className="glass-strong fixed top-0 right-0 bottom-0 z-40 flex w-[min(860px,94vw)] flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none"
                  {...sheetMotion(!animate)}
                >
                  {/* The game's item pictures for every row, card and dialog of the panel. */}
                  <PlusItemArt
                    minecraft={account.version === "auto" ? null : account.version}
                    server={serverLabel(account.server)}
                  >
                    {/* Another account (a notice opens it in place) starts afresh: no card, typed
                        text, form or picker of the one before. */}
                    <PanelBody key={account.id} account={account} onClose={closePanel} />
                  </PlusItemArt>
                </GatedMotion.aside>
              </Dialog.Content>
            </Dialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}
