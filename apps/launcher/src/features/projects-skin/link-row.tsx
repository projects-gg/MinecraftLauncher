// The account center's "Projects eşlemesi" row, right under the Discord row and like it (56 px,
// the same ring; skin1.md §13.4). Shown while signed in to friends and the service links.
// - Linked: the linked account's face, "Projects eşlemesi", its name; "…" (and a right click)
//   holds "Görünüm" for a username account and "Eşlemeyi kaldır".
// - Not linked: "Projects hesabını eşle"; a press opens the row in place (the content fades in,
//   the rows below glide): the account to link (the active one first) and the code box. Esc
//   closes it.
// - Just linked: a username account gets "Projects skinini ayarla" for 6 s, then the quiet row.
import { CaretDown, DotsThree, TShirt } from "@phosphor-icons/react";
import {
  Button,
  ContentIcon,
  ContextMenu,
  cn,
  IconButton,
  Menu,
  type MenuItem,
  PlayerHead,
  Segmented,
  Select,
  Skeleton,
  scrollBehavior,
  springSnappy,
} from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AccountFace } from "@/features/accounts/account-face";
import { useAccountsUi } from "@/features/accounts/store";
import { useAccounts } from "@/features/accounts/use-accounts";
import { useSocialUi } from "@/features/social/store";
import type { AccountSummary } from "@/lib/api";
import { projectsSkinApi } from "@/lib/api/projects-skin-api";
import type { ProjectsLink } from "@/lib/api/projects-skin-types";
import { UnlinkDialog } from "./link-area";
import { LinkCode } from "./link-code";
import { PROJECTS_SKIN_KEY, useProjectsSkinOverview } from "./queries";
import { needsCheck, SET_SKIN_OFFER_MS } from "./rules";
import { COMMAND_SLOT, CommandText } from "./text";

const rowClass =
  "flex min-h-14 w-full items-center gap-3 rounded-[var(--radius-panel)] px-3.5 py-2.5 text-left";

const fade = {
  initial: { opacity: 0 },
  animate: { opacity: 1, transition: { duration: 0.2, delay: 0.06 } },
  exit: { opacity: 0, transition: { duration: 0.14 } },
};

/** Up to three short names fit a segmented control in the 480 px sheet; more get a select. */
function fitsSegmented(accounts: readonly AccountSummary[]): boolean {
  const letters = accounts.reduce((sum, account) => sum + account.username.length, 0);
  return accounts.length <= 3 && letters <= 30;
}

function SkeletonRow() {
  const { t } = useTranslation();
  return (
    <div role="status" className={cn(rowClass, "ring-1 ring-white/[0.07]")}>
      <span className="sr-only">{t("projectsSkin.link.checking")}</span>
      <Skeleton className="size-9 rounded-lg" />
      <span className="flex min-w-0 flex-1 flex-col gap-2">
        <Skeleton className="h-3.5 w-36" />
        <Skeleton className="h-3 w-24" />
      </span>
    </div>
  );
}

function LinkedRow({
  name,
  account,
  offer,
  skinsShown,
  onAppearance,
  onRemove,
}: {
  name: string;
  /** The launcher account with the linked name, if there is one. */
  account: AccountSummary | null;
  /** "Projects skinini ayarla" is on offer (just linked). */
  offer: boolean;
  skinsShown: boolean;
  onAppearance: (id: string) => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  const appearanceFor = account && account.kind === "offline" && skinsShown ? account.id : null;
  const items: Array<MenuItem | null> = [
    appearanceFor
      ? {
          label: t("projectsSkin.row.appearance"),
          icon: <TShirt size={16} />,
          onSelect: () => onAppearance(appearanceFor),
        }
      : null,
    { label: t("projectsSkin.link.remove"), danger: true, onSelect: onRemove },
  ];
  return (
    <ContextMenu items={items}>
      <div className={cn(rowClass, "ring-1 ring-white/[0.07]")}>
        {account ? (
          <AccountFace account={account} size={36} />
        ) : (
          <PlayerHead name={name} src={null} size={36} />
        )}
        <span className="min-w-0 flex-1">
          <span className="block text-[14px] font-medium text-bone-50">
            {t("projectsSkin.row.title")}
          </span>
          <span className="block truncate text-[12.5px] text-ash-400" data-selectable>
            {name}
          </span>
        </span>
        <AnimatePresence initial={false} mode="popLayout">
          {offer && appearanceFor ? (
            <motion.span key="offer" {...fade}>
              <Button
                size="sm"
                variant="secondary"
                icon={<TShirt size={15} />}
                onClick={() => onAppearance(appearanceFor)}
              >
                {t("projectsSkin.row.setSkin")}
              </Button>
            </motion.span>
          ) : (
            <motion.span key="more" {...fade}>
              <Menu
                items={items}
                trigger={
                  <IconButton
                    size="sm"
                    label={t("projectsSkin.link.more")}
                    icon={<DotsThree size={18} weight="bold" />}
                  />
                }
              />
            </motion.span>
          )}
        </AnimatePresence>
      </div>
    </ContextMenu>
  );
}

function UnlinkedRow({
  accounts,
  expanded,
  onToggle,
  onSuccess,
  onLinked,
}: {
  accounts: readonly AccountSummary[];
  expanded: boolean;
  onToggle: (open: boolean) => void;
  onSuccess: () => void;
  onLinked: (accountId: string) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const toggleRef = useRef<HTMLButtonElement>(null);
  const formRef = useRef<HTMLDivElement>(null);
  const active = accounts.find((account) => account.active) ?? accounts[0];
  const [pick, setPick] = useState(active?.id ?? "");
  const picked = accounts.find((account) => account.id === pick) ?? active;
  const ordered = active ? [active, ...accounts.filter((a) => a.id !== active.id)] : [...accounts];

  // The row sits at the end of the list: opened, it brings its cells and their line into view.
  useEffect(() => {
    if (!expanded) return;
    const frame = requestAnimationFrame(() =>
      formRef.current?.scrollIntoView({ block: "nearest", behavior: scrollBehavior() }),
    );
    return () => cancelAnimationFrame(frame);
  }, [expanded]);

  const close = () => {
    onToggle(false);
    requestAnimationFrame(() => toggleRef.current?.focus());
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    close();
  };

  return (
    <div
      data-link-form={expanded ? "" : undefined}
      className={cn(
        "rounded-[var(--radius-panel)] ring-1 transition-[background-color,box-shadow] duration-200",
        expanded
          ? "bg-white/[0.03] ring-white/12"
          : "ring-white/[0.07] hover:bg-white/[0.05] hover:ring-white/12",
      )}
    >
      <button
        ref={toggleRef}
        type="button"
        aria-expanded={expanded}
        onClick={() => onToggle(!expanded)}
        className={cn(
          rowClass,
          "group cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
        )}
      >
        <span
          aria-hidden
          className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-obsidian-800/90"
        >
          <ContentIcon kind="account" size={16} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-[14px] font-medium text-bone-50">
            {t("projectsSkin.row.linkTitle")}
          </span>
          <span className="block text-[12.5px] text-ash-400">
            <CommandText text={t("projectsSkin.row.linkBody", { command: COMMAND_SLOT })} />
          </span>
        </span>
        <CaretDown
          size={16}
          aria-hidden
          className={cn(
            "shrink-0 text-ash-400 transition-[rotate,color] duration-200 group-hover:text-bone-100",
            expanded ? "rotate-180" : "rotate-0",
          )}
        />
      </button>
      <AnimatePresence initial={false}>
        {expanded && picked ? (
          <motion.div
            ref={formRef}
            key="form"
            {...fade}
            className="px-3.5 pt-1 pb-3"
            onKeyDown={onKeyDown}
          >
            {ordered.length > 1 ? (
              <div className="mb-3 flex items-center justify-between gap-4">
                <span className="text-[12.5px] text-ash-400">{t("projectsSkin.row.account")}</span>
                {fitsSegmented(ordered) ? (
                  <Segmented<string>
                    value={picked.id}
                    label={t("projectsSkin.row.account")}
                    onValueChange={setPick}
                    options={ordered.map((account) => ({
                      value: account.id,
                      label: account.username,
                    }))}
                  />
                ) : (
                  <Select<string>
                    value={picked.id}
                    label={t("projectsSkin.row.account")}
                    onValueChange={setPick}
                    className="w-[220px]"
                    options={ordered.map((account) => ({
                      value: account.id,
                      label: account.username,
                    }))}
                  />
                )}
              </div>
            ) : null}
            <LinkCode
              key={picked.id}
              accountId={picked.id}
              autoFocus
              onSuccess={onSuccess}
              onLinked={() => onLinked(picked.id)}
              onOther={() => void queryClient.invalidateQueries({ queryKey: PROJECTS_SKIN_KEY })}
            />
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

function RowBody({ link, skinsShown }: { link: ProjectsLink | null; skinsShown: boolean }) {
  const queryClient = useQueryClient();
  const { data: accounts = [] } = useAccounts();
  const openAppearance = useAccountsUi((s) => s.openAppearance);
  const wanted = useAccountsUi((s) => s.linkRowWanted);
  const wantLinkRow = useAccountsUi((s) => s.wantLinkRow);
  const ref = useRef<HTMLDivElement>(null);
  const [expanded, setExpanded] = useState(false);
  /** The code box shows its gold plate a moment after the link arrived. */
  const [held, setHeld] = useState(false);
  const [offer, setOffer] = useState<string | null>(null);
  const [unlinkOpen, setUnlinkOpen] = useState(false);

  // Asked once when the row shows: a link not known lately.
  // biome-ignore lint/correctness/useExhaustiveDependencies: once per showing
  useEffect(() => {
    if (needsCheck(link, Math.floor(Date.now() / 1000))) {
      void projectsSkinApi.linkCheck().catch(() => {});
    }
  }, []);

  useEffect(() => {
    if (offer === null) return;
    const timer = window.setTimeout(() => setOffer(null), SET_SKIN_OFFER_MS);
    return () => window.clearTimeout(timer);
  }, [offer]);

  const linked = link?.state === "linked" || (link?.state === "unknown" && Boolean(link.name));
  // The palette asked for the row: it opens (or shows the link) once the center is up.
  useEffect(() => {
    if (!wanted || !link || (link.state === "unknown" && !link.name)) return;
    wantLinkRow(false);
    if (!linked) setExpanded(true);
    window.setTimeout(
      () => ref.current?.scrollIntoView({ block: "center", behavior: scrollBehavior() }),
      320,
    );
  }, [wanted, link, linked, wantLinkRow]);

  const name = link?.name ?? "";
  const account =
    accounts.find((candidate) => candidate.username.toLowerCase() === name.toLowerCase()) ?? null;

  return (
    <motion.div ref={ref} layout="position" transition={springSnappy}>
      <AnimatePresence initial={false} mode="popLayout">
        {linked && !held ? (
          <motion.div key="linked" {...fade}>
            <LinkedRow
              name={name}
              account={account}
              offer={offer !== null && offer === account?.id}
              skinsShown={skinsShown}
              onAppearance={openAppearance}
              onRemove={() => setUnlinkOpen(true)}
            />
          </motion.div>
        ) : link?.state === "none" || held ? (
          <motion.div key="unlinked" {...fade}>
            <UnlinkedRow
              accounts={accounts}
              expanded={expanded || held}
              onToggle={setExpanded}
              onSuccess={() => setHeld(true)}
              onLinked={(accountId) => {
                setHeld(false);
                setExpanded(false);
                const linkedAccount = accounts.find((candidate) => candidate.id === accountId);
                if (linkedAccount?.kind === "offline" && skinsShown) setOffer(accountId);
                void queryClient.invalidateQueries({ queryKey: PROJECTS_SKIN_KEY });
              }}
            />
          </motion.div>
        ) : (
          <motion.div key="checking" {...fade}>
            <SkeletonRow />
          </motion.div>
        )}
      </AnimatePresence>
      <UnlinkDialog open={unlinkOpen} movingFrom={null} onOpenChange={setUnlinkOpen} />
    </motion.div>
  );
}

/** Shown only while signed in to friends, the feature is on and the service links. */
export function ProjectsLinkRow() {
  const signedIn = useSocialUi((s) => s.view.auth === "signedIn");
  const overview = useProjectsSkinOverview();
  const data = overview.data;
  if (!signedIn || !data?.shown || !data.service?.link) return null;
  return <RowBody link={data.link} skinsShown={data.service.skins} />;
}
