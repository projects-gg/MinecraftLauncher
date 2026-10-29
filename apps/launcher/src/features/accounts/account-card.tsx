// One account in the account center: face, name, type, activity and its actions. The whole
// left part switches to the account; actions sit on the right (edit or appearance, pin, remove).
// Offline accounts edit in place: name, and the picture behind the face button; while Projects
// skins are offered (SKIN1) their T-shirt opens the Projects skin panel, before the pencil.
import {
  Check,
  PencilSimple,
  PushPin,
  SignOut,
  Trash,
  TShirt,
  WarningCircle,
  X,
} from "@phosphor-icons/react";
import { Button, Chip, cn, easeOutExpo, IconButton, springSnappy, useReduceMotion } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type FormEvent, type KeyboardEvent, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { MicrosoftMark } from "@/features/settings/microsoft-mark";
import type { AccountSummary, AfkAccount } from "@/lib/api";
import { formatRelative } from "@/lib/format";
import { AccountAvatar } from "./account-avatar";
import { AvatarChoiceButton } from "./avatar-choice";
import { useCalm } from "./calm";
import { checkOfflineName } from "./name-rules";

export interface AccountCardProps {
  account: AccountSummary;
  /** Every account, for the duplicate check of a rename. */
  accounts: readonly AccountSummary[];
  /** The online AFK session with this name, if any. */
  afk: AfkAccount | null;
  /** The game on screen plays with this account (PLAY-ACCT): "Oyunda". */
  playing?: boolean;
  bracketsId: string;
  onSwitch: () => void;
  onRemove: () => void;
  /** Resolves to an error message, or null once saved. */
  onRename: (name: string) => Promise<string | null>;
  onSkins: () => void;
  /** Username accounts while Projects skins are offered (SKIN1): the T-shirt's panel. */
  onProjectsSkin?: () => void;
  onRelogin: () => void;
  onPin: () => void;
  /** Offline accounts: a bundled picture, or null for the head. */
  onAvatar: (avatar: string | null) => void;
  /** Just added, signed in or brought back: a brief gold outline says "this one". */
  fresh?: boolean;
  onFreshDone?: () => void;
}

const actionClass = "size-[34px] rounded-lg";

/**
 * "Sabitle": a pin that presses in (fills, sits on the quiet plate, IconButton's active) when pinned
 * and pops out when not. Bone, not gold: a pin is a choice (design.md §6).
 */
function PinButton({ pinned, onToggle }: { pinned: boolean; onToggle: () => void }) {
  const { t } = useTranslation();
  const reduced = useReduceMotion();
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
  }, []);
  return (
    <IconButton
      label={pinned ? t("accounts.center.unpin") : t("accounts.center.pin")}
      active={pinned}
      className={actionClass}
      onClick={onToggle}
      icon={
        <motion.span
          key={pinned ? "pinned" : "loose"}
          className="flex"
          initial={mounted.current && !reduced ? { scale: 0.6, rotate: pinned ? -35 : 25 } : false}
          animate={{ scale: 1, rotate: 0 }}
          transition={springSnappy}
        >
          <PushPin size={18} weight={pinned ? "fill" : "regular"} />
        </motion.span>
      }
    />
  );
}

/** "Microsoft" on Microsoft accounts; an account with only a username has no chip (KEYS2). */
export function KindChip({ kind }: { kind: AccountSummary["kind"] }) {
  const { t } = useTranslation();
  if (kind !== "microsoft") return null;
  return (
    <Chip className="shrink-0 gap-1.5 font-medium text-ash-300">
      <MicrosoftMark size={10} />
      {t("accounts.kind.microsoft")}
    </Chip>
  );
}

function RenameForm({
  account,
  accounts,
  onSave,
  onDone,
}: {
  account: AccountSummary;
  accounts: readonly AccountSummary[];
  onSave: (name: string) => Promise<string | null>;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const messageId = useId();
  const [value, setValue] = useState(account.username);
  const [final, setFinal] = useState(false);
  const [serverError, setServerError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const check = checkOfflineName(value, accounts, { selfId: account.id, final });
  const problem = serverError ?? (check.problem ? t(`accounts.name.${check.problem}`) : null);

  const submit = async (event?: FormEvent) => {
    event?.preventDefault();
    const strict = checkOfflineName(value, accounts, { selfId: account.id, final: true });
    setFinal(true);
    if (strict.problem || saving) return;
    if (strict.name === account.username) return onDone();
    setSaving(true);
    const error = await onSave(strict.name);
    setSaving(false);
    if (error) setServerError(error);
    else onDone();
  };

  return (
    <form onSubmit={(event) => void submit(event)} className="min-w-0 flex-1">
      <div className="flex items-center gap-1">
        <input
          // biome-ignore lint/a11y/noAutofocus: the player just asked to rename this account
          autoFocus
          value={value}
          onChange={(event) => {
            setValue(event.target.value);
            setServerError(null);
            setFinal(false);
          }}
          onFocus={(event) => event.currentTarget.select()}
          aria-label={t("accounts.center.renameLabel")}
          aria-invalid={Boolean(problem) || undefined}
          aria-describedby={messageId}
          autoComplete="off"
          spellCheck={false}
          className={cn(
            "h-[36px] min-w-0 flex-1 rounded-md border bg-obsidian-800/90 px-3 font-display text-[15px] font-semibold text-bone-50 transition-colors",
            "focus:ring-2 focus:outline-none",
            problem
              ? "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25"
              : "border-white/12 focus:border-gold-400/70 focus:ring-gold-400/25",
          )}
        />
        <IconButton
          type="submit"
          label={t("accounts.center.renameSave")}
          icon={<Check size={18} weight="bold" />}
          className={cn(actionClass, "text-gold-300")}
          disabled={saving}
        />
        <IconButton
          label={t("accounts.center.renameCancel")}
          icon={<X size={18} />}
          className={actionClass}
          onClick={onDone}
        />
      </div>
      <p
        id={messageId}
        role={problem ? "alert" : undefined}
        className={cn(
          "mt-1.5 pr-2 text-[12.5px] leading-snug",
          problem ? "text-flare-400" : "text-ash-400",
        )}
      >
        {problem ?? t("accounts.center.renameWarning")}
        {problem && check.suggestion ? (
          <>
            {" "}
            <button
              type="button"
              className="cursor-pointer font-semibold text-gold-300 underline-offset-2 hover:underline"
              onClick={() => {
                setValue(check.suggestion ?? value);
                setServerError(null);
              }}
            >
              {t("accounts.center.useSuggestion", { name: check.suggestion })}
            </button>
          </>
        ) : null}
      </p>
    </form>
  );
}

export function AccountCard({
  account,
  accounts,
  afk,
  playing = false,
  bracketsId,
  onSwitch,
  onRemove,
  onRename,
  onSkins,
  onProjectsSkin,
  onRelogin,
  onPin,
  onAvatar,
  fresh = false,
  onFreshDone,
}: AccountCardProps) {
  const { t } = useTranslation();
  const calm = useCalm();
  const [editing, setEditing] = useState(false);
  const mainRef = useRef<HTMLButtonElement>(null);
  const microsoft = account.kind === "microsoft";
  const expired = microsoft && Boolean(account.needsRelogin);

  const stopEditing = () => {
    setEditing(false);
    requestAnimationFrame(() => mainRef.current?.focus());
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (editing || event.target !== mainRef.current) return;
    if (event.key === "Delete") {
      event.preventDefault();
      onRemove();
    } else if (event.key === "F2" && !microsoft) {
      event.preventDefault();
      setEditing(true);
    }
  };

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: keyboard shortcuts of the card's own buttons bubble here
    <div
      onKeyDown={onKeyDown}
      className={cn(
        "group/card relative rounded-[var(--radius-panel)] ring-1 transition-[background-color,box-shadow] duration-200",
        account.active
          ? "bg-gold-500/[0.07] ring-gold-400/30"
          : "bg-white/[0.03] ring-white/[0.07] hover:bg-white/[0.055] hover:ring-white/12",
        "has-[[data-account-main]:focus-visible]:ring-2 has-[[data-account-main]:focus-visible]:ring-gold-300",
      )}
    >
      <AnimatePresence>
        {fresh ? (
          <motion.span
            key="fresh"
            aria-hidden
            className="pointer-events-none absolute -inset-px rounded-[var(--radius-panel)] ring-2 ring-gold-300/70"
            initial={{ opacity: 0 }}
            animate={{ opacity: [0, 1, 0] }}
            transition={{ duration: 1.6, times: [0, 0.18, 1], ease: "easeOut", delay: 0.1 }}
            onAnimationComplete={onFreshDone}
          />
        ) : null}
      </AnimatePresence>
      <div className="flex items-center gap-1 pr-2">
        {editing ? (
          // Escape anywhere in the editor (name field or face button) ends editing, not the
          // sheet. Keys from the picture popover only bubble here through React: ignore them.
          // biome-ignore lint/a11y/noStaticElementInteractions: Escape from the editor's own controls
          <div
            data-rename-form
            className="flex min-w-0 flex-1 items-start gap-3 py-2.5 pl-3"
            onKeyDown={(event) => {
              if (event.key !== "Escape") return;
              if (!event.currentTarget.contains(event.target as Node)) return;
              event.preventDefault();
              event.stopPropagation();
              stopEditing();
            }}
          >
            <AvatarChoiceButton account={account} bracketsId={bracketsId} onChange={onAvatar} />
            <RenameForm
              account={account}
              accounts={accounts}
              onSave={onRename}
              onDone={stopEditing}
            />
          </div>
        ) : (
          <>
            <button
              ref={mainRef}
              type="button"
              data-account-main
              aria-current={account.active ? "true" : undefined}
              aria-label={
                account.active
                  ? t("accounts.center.isActive", { name: account.username })
                  : t("accounts.center.switchTo", { name: account.username })
              }
              onClick={() => {
                if (!account.active) onSwitch();
              }}
              className={cn(
                "flex min-w-0 flex-1 items-center gap-3 py-2.5 pl-3 text-left outline-none",
                account.active ? "cursor-default" : "cursor-pointer",
              )}
            >
              <AccountAvatar account={account} selected={account.active} bracketsId={bracketsId} />
              <span className="min-w-0 flex-1">
                <span className="flex min-w-0 items-center gap-2">
                  <span className="truncate font-display text-[15px] leading-tight font-semibold text-bone-50">
                    {account.username}
                  </span>
                  <AnimatePresence initial={false}>
                    {account.active ? (
                      <motion.span
                        key="active"
                        className="flex shrink-0"
                        initial={calm({ opacity: 0, scale: 0.9 })}
                        animate={calm({ opacity: 1, scale: 1 })}
                        exit={calm({ opacity: 0, scale: 0.9, transition: { duration: 0.12 } })}
                        transition={{ duration: 0.24, ease: easeOutExpo }}
                      >
                        <Chip>{t("accounts.center.active")}</Chip>
                      </motion.span>
                    ) : null}
                  </AnimatePresence>
                  {playing ? (
                    <Chip tone="jade" className="shrink-0">
                      {t("playAcct.inGame")}
                    </Chip>
                  ) : null}
                  {afk ? (
                    <Chip tone="jade" className="shrink-0">
                      {t("accounts.center.afk")}
                    </Chip>
                  ) : null}
                </span>
                <span className="mt-1.5 flex min-w-0 items-center gap-2 text-[12.5px] text-ash-400">
                  <KindChip kind={account.kind} />
                  <span className="truncate">
                    {account.lastUsedAt
                      ? t("accounts.center.lastPlayed", {
                          when: formatRelative(account.lastUsedAt),
                        })
                      : t("accounts.center.neverPlayed")}
                  </span>
                </span>
              </span>
            </button>
            {microsoft ? (
              <IconButton
                label={t("accounts.center.skins")}
                icon={<TShirt size={18} />}
                className={actionClass}
                onClick={onSkins}
              />
            ) : (
              <>
                {onProjectsSkin ? (
                  <IconButton
                    label={t("accounts.center.skins")}
                    icon={<TShirt size={18} />}
                    className={actionClass}
                    onClick={onProjectsSkin}
                  />
                ) : null}
                <IconButton
                  label={t("accounts.center.edit")}
                  icon={<PencilSimple size={18} />}
                  className={actionClass}
                  onClick={() => setEditing(true)}
                />
              </>
            )}
            <PinButton pinned={Boolean(account.pinned)} onToggle={onPin} />
            <IconButton
              label={microsoft ? t("accounts.center.removeMicrosoft") : t("accounts.center.remove")}
              icon={microsoft ? <SignOut size={18} /> : <Trash size={18} />}
              className={cn(actionClass, "hover:bg-flare-400/12 hover:text-flare-400")}
              onClick={onRemove}
            />
          </>
        )}
      </div>
      {expired && !editing ? (
        <div className="flex items-center gap-2.5 rounded-b-[var(--radius-panel)] border-t border-flare-400/15 bg-flare-400/[0.06] py-1.5 pr-2 pl-3.5">
          <WarningCircle size={17} weight="fill" className="shrink-0 text-flare-400" aria-hidden />
          <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-flare-400">
            {t("accounts.center.expired")}
          </span>
          <Button
            variant="secondary"
            size="sm"
            className="h-[32px]"
            icon={<MicrosoftMark size={13} />}
            onClick={onRelogin}
          >
            {t("accounts.center.relogin")}
          </Button>
        </div>
      ) : null}
    </div>
  );
}
